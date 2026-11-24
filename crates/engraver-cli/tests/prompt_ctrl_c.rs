//! Ctrl+C at a yes/no prompt, on a real pseudo-terminal.
//!
//! The prompt must answer "cancelled" and nothing else. In particular the
//! keypress must not turn into a signal that reaches the process's Ctrl+C
//! handler, which prints its own message on its own thread and races the one
//! the caller prints. On Windows the keypress must not be swallowed by that
//! handler and leave the prompt waiting.
//!
//! No command reaches a prompt without root and a drive, so these tests
//! compile `src/prompt.rs` and run `prompt::ask` itself. The test binary runs
//! itself as the child (the `prompt_child_process` test), with a pty on Unix
//! and a ConPTY on Windows as its terminal, and a `ctrlc` handler installed
//! as in `engraver`.

#![allow(unsafe_code)]

#[path = "../src/prompt.rs"]
mod prompt;

use std::io::Write;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

/// Set in the child. `yes` or `no` is the prompt's default.
const CHILD_ENV: &str = "ENGRAVER_PROMPT_TEST_CHILD";
/// The test that the parent re-runs as the child.
const CHILD_TEST: &str = "prompt_child_process";
/// A single word, so a terminal that redraws the text cannot split it.
const PROMPT: &str = "proceed?";
/// What the child's Ctrl+C handler prints.
const HANDLER_RAN: &str = "handler ran";

const EXIT_YES: i32 = 10;
const EXIT_NO: i32 = 11;
const EXIT_CANCELLED: i32 = 130;
const EXIT_ERROR: i32 = 2;

/// How long the parent waits for the child to show the prompt, and again for
/// it to exit. A prompt that keeps waiting fails the test instead of hanging.
const TIMEOUT: Duration = Duration::from_secs(10);

/// How long the child waits after the prompt returns, so that a signal the
/// prompt raised has time to reach the handler's thread.
const SIGNAL_GRACE: Duration = Duration::from_millis(300);

/// The child side. It does nothing unless the parent started it.
#[test]
fn prompt_child_process() {
    let Ok(default) = std::env::var(CHILD_ENV) else {
        return;
    };

    ctrlc::set_handler(|| {
        let mut out = std::io::stdout();
        let _ = writeln!(out, "{HANDLER_RAN}");
        let _ = out.flush();
    })
    .expect("install the Ctrl+C handler");

    let asked = prompt::ask(PROMPT, default == "yes");
    std::thread::sleep(SIGNAL_GRACE);

    let (line, code) = match asked {
        Ok(Some(true)) => ("result: yes".to_string(), EXIT_YES),
        Ok(Some(false)) => ("result: no".to_string(), EXIT_NO),
        Ok(None) => ("result: cancelled".to_string(), EXIT_CANCELLED),
        Err(err) => (format!("result: error: {err}"), EXIT_ERROR),
    };
    let mut out = std::io::stdout();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
    std::process::exit(code);
}

enum Exit {
    Code(i32),
    /// The child never got as far as the step named here.
    TimedOut(&'static str),
}

impl Exit {
    fn describe(&self) -> String {
        match self {
            Self::Code(code) => format!("exited {code}"),
            Self::TimedOut(step) => format!("timed out waiting to {step}"),
        }
    }
}

struct Run {
    exit: Exit,
    /// Everything the child wrote to the terminal.
    output: String,
}

/// One terminal at a time keeps a slow runner from starving a child.
fn serial() -> MutexGuard<'static, ()> {
    static SERIAL: Mutex<()> = Mutex::new(());
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

fn assert_exit(run: &Run, expected: i32, what: &str) {
    match run.exit {
        Exit::Code(code) if code == expected => {}
        ref other => panic!(
            "{what}: expected exit {expected}, but the child {}\nterminal output: {:?}",
            other.describe(),
            run.output
        ),
    }
}

#[test]
fn ctrl_c_cancels_the_prompt_and_wakes_no_signal_handler() {
    let _guard = serial();

    let run = pty::run(false, b"\x03");

    assert!(
        !run.output.contains(HANDLER_RAN),
        "Ctrl+C at the prompt reached the process's Ctrl+C handler (the child {})\n\
         terminal output: {:?}",
        run.exit.describe(),
        run.output
    );
    assert_exit(&run, EXIT_CANCELLED, "Ctrl+C at the prompt");
}

#[test]
fn n_declines() {
    let _guard = serial();
    assert_exit(&pty::run(true, b"n"), EXIT_NO, "n");
}

#[test]
fn y_accepts() {
    let _guard = serial();
    assert_exit(&pty::run(false, b"y"), EXIT_YES, "y");
}

#[test]
fn upper_case_answers_count() {
    let _guard = serial();
    assert_exit(&pty::run(true, b"N"), EXIT_NO, "N");
    assert_exit(&pty::run(false, b"Y"), EXIT_YES, "Y");
}

#[test]
fn enter_takes_a_default_of_no() {
    let _guard = serial();
    assert_exit(&pty::run(false, b"\r"), EXIT_NO, "Enter with default no");
}

#[test]
fn enter_takes_a_default_of_yes() {
    let _guard = serial();
    assert_exit(&pty::run(true, b"\r"), EXIT_YES, "Enter with default yes");
}

#[test]
fn other_keys_are_ignored() {
    let _guard = serial();
    assert_exit(&pty::run(false, b"xq y"), EXIT_YES, "keys before y");
}

#[cfg(unix)]
mod pty {
    use super::{Exit, Run, CHILD_ENV, CHILD_TEST, PROMPT, TIMEOUT};
    use std::fs::File;
    use std::io::{Read, Write};
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
    use std::os::unix::process::CommandExt;
    use std::process::{Child, Command, Stdio};
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    /// Run the child on a pty, wait for its prompt, send `keys`, and wait for
    /// it to exit.
    pub fn run(default_yes: bool, keys: &[u8]) -> Run {
        let (master, slave) = open_pty();
        let mut child = spawn(slave, default_yes);

        let output = Arc::new(Mutex::new(Vec::<u8>::new()));
        let drained = drain(&master, &output);
        let text = || String::from_utf8_lossy(&output.lock().unwrap()).into_owned();

        let deadline = Instant::now() + TIMEOUT;
        let exit = (|| {
            if !wait_for(deadline, || text().contains(PROMPT)) {
                return Exit::TimedOut("show the prompt");
            }
            // The prompt reads a key in raw mode. A key sent earlier meets the
            // line discipline's cooked mode, where Ctrl+C is a signal.
            if !wait_for(deadline, || !is_canonical(&master)) {
                return Exit::TimedOut("start reading a key");
            }
            (&master).write_all(keys).expect("send the keys");
            match wait_for_exit(&mut child, deadline) {
                Some(code) => Exit::Code(code),
                None => Exit::TimedOut("exit"),
            }
        })();
        if matches!(exit, Exit::TimedOut(_)) {
            let _ = child.kill();
            let _ = child.wait();
        }

        // The pty reports the end of the child's output once nothing holds
        // its slave side open any more.
        let _ = drained.recv_timeout(Duration::from_secs(2));
        Run {
            exit,
            output: text(),
        }
    }

    fn open_pty() -> (File, OwnedFd) {
        let (mut master, mut slave) = (-1, -1);
        let rc = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        assert_eq!(rc, 0, "openpty: {}", std::io::Error::last_os_error());
        unsafe {
            libc::fcntl(master, libc::F_SETFD, libc::FD_CLOEXEC);
            (File::from_raw_fd(master), OwnedFd::from_raw_fd(slave))
        }
    }

    /// Start this test binary as the child, with `slave` as its terminal.
    fn spawn(slave: OwnedFd, default_yes: bool) -> Child {
        let mut command = Command::new(std::env::current_exe().expect("test binary path"));
        command
            .args(["--exact", CHILD_TEST, "--nocapture", "--test-threads=1"])
            .env(CHILD_ENV, if default_yes { "yes" } else { "no" })
            .stdin(Stdio::from(slave.try_clone().expect("dup the pty")))
            .stdout(Stdio::from(slave.try_clone().expect("dup the pty")))
            .stderr(Stdio::from(slave));
        // Make the pty the child's controlling terminal, as a shell does.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::ioctl(0, libc::TIOCSCTTY as _, 0 as libc::c_int) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        // Dropping `command` closes the parent's copies of the slave.
        command.spawn().expect("start the child")
    }

    /// Collect the child's output on a thread. The returned channel receives
    /// a message when there is no more.
    fn drain(master: &File, output: &Arc<Mutex<Vec<u8>>>) -> mpsc::Receiver<()> {
        let mut reader = master.try_clone().expect("dup the pty master");
        let output = Arc::clone(output);
        let (done, drained) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) => break,
                    Ok(n) => output.lock().unwrap().extend_from_slice(&buf[..n]),
                    Err(err) if err.kind() == std::io::ErrorKind::Interrupted => {}
                    // Linux reports the slave's close as EIO.
                    Err(_) => break,
                }
            }
            let _ = done.send(());
        });
        drained
    }

    /// Whether the terminal is in canonical (line) mode. The master reads the
    /// slave's settings.
    fn is_canonical(master: &File) -> bool {
        let mut termios = unsafe { std::mem::zeroed::<libc::termios>() };
        let rc = unsafe { libc::tcgetattr(master.as_raw_fd(), &mut termios) };
        assert_eq!(rc, 0, "tcgetattr: {}", std::io::Error::last_os_error());
        termios.c_lflag & libc::ICANON != 0
    }

    fn wait_for(deadline: Instant, mut ready: impl FnMut() -> bool) -> bool {
        while !ready() {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        true
    }

    fn wait_for_exit(child: &mut Child, deadline: Instant) -> Option<i32> {
        let mut status = None;
        wait_for(deadline, || {
            status = child.try_wait().expect("poll the child");
            status.is_some()
        });
        // A child ended by a signal has no exit code.
        status.map(|status| status.code().unwrap_or(-1))
    }
}

#[cfg(windows)]
mod pty {
    use super::{Exit, Run, CHILD_ENV, CHILD_TEST, PROMPT, TIMEOUT};
    use std::ffi::{c_void, OsString};
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::{null, null_mut};
    use std::sync::mpsc;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0};
    use windows_sys::Win32::Storage::FileSystem::{ReadFile, WriteFile};
    use windows_sys::Win32::System::Console::{
        ClosePseudoConsole, CreatePseudoConsole, COORD, HPCON,
    };
    use windows_sys::Win32::System::Pipes::CreatePipe;
    use windows_sys::Win32::System::Threading::{
        CreateProcessW, DeleteProcThreadAttributeList, GetExitCodeProcess,
        InitializeProcThreadAttributeList, TerminateProcess, UpdateProcThreadAttribute,
        WaitForSingleObject, CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT,
        PROCESS_INFORMATION, PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE, STARTUPINFOEXW,
    };

    /// How long after the prompt shows before a key is sent. The prompt is
    /// already reading by then. A Ctrl+C that arrives before it turns off
    /// `ENABLE_PROCESSED_INPUT` is a console control event, not a key.
    const SETTLE: Duration = Duration::from_millis(1000);

    /// A device status report request, which a terminal answers.
    const CURSOR_POSITION_QUERY: &[u8] = b"\x1b[6n";
    const CURSOR_POSITION_REPLY: &[u8] = b"\x1b[1;1R";

    struct Handle(HANDLE);

    // A handle is a process-wide token, not tied to the thread that has it.
    unsafe impl Send for Handle {}
    unsafe impl Sync for Handle {}

    impl Handle {
        /// A method, so a closure captures the whole `Handle` and not the
        /// raw pointer inside it, which is not `Send`.
        fn raw(&self) -> HANDLE {
            self.0
        }
    }

    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    fn pipe() -> (Handle, Handle) {
        let (mut read, mut write): (HANDLE, HANDLE) = (null_mut(), null_mut());
        let ok = unsafe { CreatePipe(&mut read, &mut write, null(), 0) };
        assert_ne!(ok, 0, "CreatePipe: {}", std::io::Error::last_os_error());
        (Handle(read), Handle(write))
    }

    fn write_all(to: &Handle, bytes: &[u8]) {
        let mut written = 0;
        let ok = unsafe {
            WriteFile(
                to.0,
                bytes.as_ptr(),
                bytes.len() as u32,
                &mut written,
                null_mut(),
            )
        };
        assert_ne!(ok, 0, "WriteFile: {}", std::io::Error::last_os_error());
    }

    /// Run the child on a ConPTY, wait for its prompt, send `keys`, and wait
    /// for it to exit.
    pub fn run(default_yes: bool, keys: &[u8]) -> Run {
        let (input_read, input) = pipe();
        let (output_read, output_write) = pipe();
        let input = Arc::new(input);

        let mut console: HPCON = 0;
        let size = COORD { X: 120, Y: 30 };
        let hr =
            unsafe { CreatePseudoConsole(size, input_read.0, output_write.0, 0, &mut console) };
        assert_eq!(hr, 0, "CreatePseudoConsole: HRESULT {hr:#x}");
        // The pseudo console holds its own copies of these two ends.
        drop(input_read);
        drop(output_write);

        let process = spawn(console, default_yes);

        let output = Arc::new(Mutex::new(Vec::<u8>::new()));
        let drained = drain(output_read, &output, &input);
        let text = || String::from_utf8_lossy(&output.lock().unwrap()).into_owned();

        let deadline = Instant::now() + TIMEOUT;
        let exit = (|| {
            if !wait_for(deadline, || text().contains(PROMPT)) {
                return Exit::TimedOut("show the prompt");
            }
            std::thread::sleep(SETTLE);
            write_all(&input, keys);
            match wait_for_exit(&process, deadline) {
                Some(code) => Exit::Code(code),
                None => Exit::TimedOut("exit"),
            }
        })();
        if matches!(exit, Exit::TimedOut(_)) {
            unsafe { TerminateProcess(process.0, 1) };
        }

        close_console(console);
        let _ = drained.recv_timeout(Duration::from_secs(2));
        Run {
            exit,
            output: text(),
        }
    }

    /// Start this test binary as the child, attached to `console`.
    fn spawn(console: HPCON, default_yes: bool) -> Handle {
        let exe = std::env::current_exe().expect("test binary path");
        let mut command_line: Vec<u16> = format!(
            "\"{}\" --exact {CHILD_TEST} --nocapture --test-threads=1",
            exe.display()
        )
        .encode_utf16()
        .chain([0])
        .collect();
        let environment = environment_block(CHILD_ENV, if default_yes { "yes" } else { "no" });

        // The attribute list ties the child to the pseudo console.
        let mut size = 0usize;
        unsafe { InitializeProcThreadAttributeList(null_mut(), 1, 0, &mut size) };
        let mut list = vec![0usize; size.div_ceil(std::mem::size_of::<usize>())];
        let list_ptr = list.as_mut_ptr().cast::<c_void>();
        let ok = unsafe { InitializeProcThreadAttributeList(list_ptr, 1, 0, &mut size) };
        assert_ne!(ok, 0, "InitializeProcThreadAttributeList");
        let ok = unsafe {
            UpdateProcThreadAttribute(
                list_ptr,
                0,
                PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE as usize,
                console as *const c_void,
                std::mem::size_of::<HPCON>(),
                null_mut(),
                null(),
            )
        };
        assert_ne!(ok, 0, "UpdateProcThreadAttribute");

        let mut startup = unsafe { std::mem::zeroed::<STARTUPINFOEXW>() };
        startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
        startup.lpAttributeList = list_ptr;
        let mut info = unsafe { std::mem::zeroed::<PROCESS_INFORMATION>() };
        let ok = unsafe {
            CreateProcessW(
                null(),
                command_line.as_mut_ptr(),
                null(),
                null(),
                0,
                EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
                environment.as_ptr().cast(),
                null(),
                &startup.StartupInfo,
                &mut info,
            )
        };
        assert_ne!(ok, 0, "CreateProcessW: {}", std::io::Error::last_os_error());
        unsafe { DeleteProcThreadAttributeList(list_ptr) };
        drop(Handle(info.hThread));
        Handle(info.hProcess)
    }

    /// This process's environment with `name` set, as the UTF-16 block
    /// `CreateProcessW` takes.
    fn environment_block(name: &str, value: &str) -> Vec<u16> {
        let mut vars: Vec<(OsString, OsString)> = std::env::vars_os()
            .filter(|(key, _)| !key.to_string_lossy().eq_ignore_ascii_case(name))
            .collect();
        vars.push((name.into(), value.into()));
        // The block is sorted by name, ignoring case.
        vars.sort_by_key(|(key, _)| key.to_string_lossy().to_uppercase());

        let mut block = Vec::new();
        for (key, value) in vars {
            block.extend(key.encode_wide());
            block.push(u16::from(b'='));
            block.extend(value.encode_wide());
            block.push(0);
        }
        block.push(0);
        block
    }

    /// Collect the child's output on a thread, and answer a cursor position
    /// query with a position so the pseudo console never waits on a reply.
    /// The returned channel receives a message when there is no more output.
    fn drain(
        read: Handle,
        output: &Arc<Mutex<Vec<u8>>>,
        input: &Arc<Handle>,
    ) -> mpsc::Receiver<()> {
        let output = Arc::clone(output);
        let input = Arc::clone(input);
        let (done, drained) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            let mut answered = 0;
            loop {
                let mut n = 0;
                let ok = unsafe {
                    ReadFile(
                        read.raw(),
                        buf.as_mut_ptr(),
                        buf.len() as u32,
                        &mut n,
                        null_mut(),
                    )
                };
                if ok == 0 || n == 0 {
                    break;
                }
                let mut seen = output.lock().unwrap();
                seen.extend_from_slice(&buf[..n as usize]);
                let queries = seen
                    .windows(CURSOR_POSITION_QUERY.len())
                    .filter(|window| *window == CURSOR_POSITION_QUERY)
                    .count();
                drop(seen);
                while answered < queries {
                    write_all(&input, CURSOR_POSITION_REPLY);
                    answered += 1;
                }
            }
            let _ = done.send(());
        });
        drained
    }

    /// Close the pseudo console on a thread. On some Windows versions the
    /// call waits for output nobody reads, and a test must not hang on it.
    fn close_console(console: HPCON) {
        let (done, closed) = mpsc::channel();
        std::thread::spawn(move || {
            unsafe { ClosePseudoConsole(console) };
            let _ = done.send(());
        });
        let _ = closed.recv_timeout(Duration::from_secs(5));
    }

    fn wait_for(deadline: Instant, mut ready: impl FnMut() -> bool) -> bool {
        while !ready() {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        true
    }

    fn wait_for_exit(process: &Handle, deadline: Instant) -> Option<i32> {
        let left = deadline.saturating_duration_since(Instant::now());
        let millis = u32::try_from(left.as_millis()).unwrap_or(u32::MAX);
        if unsafe { WaitForSingleObject(process.0, millis) } != WAIT_OBJECT_0 {
            return None;
        }
        let mut code = 0;
        let ok = unsafe { GetExitCodeProcess(process.0, &mut code) };
        assert_ne!(ok, 0, "GetExitCodeProcess");
        Some(code as i32)
    }
}
