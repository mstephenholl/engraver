//! A yes/no question on the terminal.
//!
//! This file depends on nothing else in the crate, so the integration tests
//! compile it directly with `#[path]` and drive it on a pseudo-terminal.

use std::io;

use console::{Key, Term};

/// Ask a yes/no question on the terminal and wait for an answer.
///
/// `default` is what Enter answers. Returns `Ok(None)` when the user pressed
/// Ctrl+C. Any other failure, including no terminal to ask on, is an error.
///
/// Keys come from [`Term::read_key_raw`], which hands Ctrl+C back as a key.
/// [`Term::read_key`] turns it into a signal instead. On Unix that is a
/// SIGINT the process raises on itself, which wakes the Ctrl+C handler to
/// print its own line next to the caller's. On Windows it is a console
/// control event for that handler, and the read keeps waiting.
pub(crate) fn ask(prompt: &str, default: bool) -> io::Result<Option<bool>> {
    let term = Term::stderr();
    if !term.is_term() {
        return Err(io::Error::new(
            io::ErrorKind::NotConnected,
            "not a terminal",
        ));
    }

    let choices = if default { "[Y/n]" } else { "[y/N]" };
    term.write_str(&format!("{prompt} {choices} "))?;
    term.flush()?;

    let answer = loop {
        match term.read_key_raw()? {
            Key::Char('y' | 'Y') => break true,
            Key::Char('n' | 'N') => break false,
            Key::Enter => break default,
            Key::CtrlC => return Ok(None),
            _ => {}
        }
    };

    // Replace the question with the answer.
    term.clear_line()?;
    term.write_line(&format!("{prompt} {}", if answer { "yes" } else { "no" }))?;
    Ok(Some(answer))
}
