//! Throughput and CPU cost of synchronous writes versus `IoUringDevice`.
//!
//! Writes the same data to one target, a file or a raw block device, through
//! four engines, all with O_DIRECT, then fsyncs:
//!
//! - `sync`: what `engraver write` does today. The device is opened with
//!   `block_size = chunk` and fed from a buffer that is not block aligned, so
//!   every write takes the bounce-buffer copy.
//! - `sync-aligned`: the same path from an aligned buffer, so no copy.
//! - `uring-qd1`: `write_batch` with one chunk per call.
//! - `uring-qdN`: `write_batch` with `--depth` chunks per call.
//!
//! With `--depth 1`, `uring-qdN` would repeat `uring-qd1`, so it is skipped.
//!
//! One untimed warmup pass runs first. Then each engine gets `--runs` timed
//! runs, interleaved round-robin so drift and device caches favour no engine.
//! The timed region covers every chunk write and the final sync. After each
//! run the data is read back and checked, so an engine that drops writes shows
//! up as an error instead of a win. The exit code is nonzero if any run failed.
//!
//! ```text
//! uring_bench <target> [--size N] [--chunk N] [--depth N] [--runs N] [--destroy-data]
//! ```
//!
//! Sizes take a plain byte count or a K/M/G suffix (binary). Defaults: size
//! 1G, chunk 4M, depth 8, runs 5. A file target is created and extended to
//! `--size` if needed. A block device is overwritten from offset 0, so it is
//! refused without `--destroy-data`, and it needs root:
//!
//! ```text
//! cargo build --release -p engraver-platform --example uring_bench --features io_uring
//! sudo target/release/examples/uring_bench /dev/sdX --destroy-data
//! ```

#[cfg(target_os = "linux")]
mod bench {
    use engraver_platform::{open_device, IoUringDevice, OpenOptions, RawDevice};
    use std::io::{Seek, SeekFrom, Write};
    use std::ops::Range;
    use std::os::unix::fs::FileTypeExt;
    use std::process::ExitCode;
    use std::time::{Duration, Instant};

    const ALIGN: usize = 4096;
    const GLIBC_VEC_OFFSET: usize = 16;
    const HEADER_LEN: usize = 16;
    const KIB: u64 = 1024;
    const MIB: u64 = 1024 * KIB;
    const GIB: u64 = 1024 * MIB;
    const USAGE: &str =
        "usage: uring_bench <target> [--size N] [--chunk N] [--depth N] [--runs N] [--destroy-data]";

    type R<T> = Result<T, Box<dyn std::error::Error>>;

    #[derive(Clone, Copy, PartialEq)]
    enum Engine {
        Sync,
        SyncAligned,
        Uring { depth: usize },
    }

    impl Engine {
        fn name(self) -> String {
            match self {
                Engine::Sync => "sync".into(),
                Engine::SyncAligned => "sync-aligned".into(),
                Engine::Uring { depth } => format!("uring-qd{depth}"),
            }
        }

        fn skew(self) -> usize {
            if self == Engine::Sync {
                GLIBC_VEC_OFFSET
            } else {
                0
            }
        }

        fn batch(self) -> usize {
            match self {
                Engine::Uring { depth } => depth,
                _ => 1,
            }
        }
    }

    struct Run {
        id: u64,
        engine: Engine,
        bytes: u64,
        wall: Duration,
        user: Duration,
        sys: Duration,
        error: Option<String>,
    }

    fn ratio(n: f64, d: f64) -> f64 {
        if d > 0.0 {
            n / d
        } else {
            0.0
        }
    }

    impl Run {
        fn cpu(&self) -> f64 {
            (self.user + self.sys).as_secs_f64()
        }

        fn mib_s(&self) -> f64 {
            ratio(self.bytes as f64 / MIB as f64, self.wall.as_secs_f64())
        }

        fn cpu_pct(&self) -> f64 {
            100.0 * ratio(self.cpu(), self.wall.as_secs_f64())
        }

        fn cpu_ms_per_gib(&self) -> f64 {
            ratio(self.cpu() * 1000.0, self.bytes as f64 / GIB as f64)
        }
    }

    struct Config {
        path: String,
        size: u64,
        chunk: usize,
        depth: usize,
        runs: u64,
        destroy_data: bool,
    }

    fn parse_size(s: &str) -> Result<u64, String> {
        let up = s.to_ascii_uppercase();
        let (digits, unit) = if let Some(d) = up.strip_suffix('K') {
            (d, KIB)
        } else if let Some(d) = up.strip_suffix('M') {
            (d, MIB)
        } else if let Some(d) = up.strip_suffix('G') {
            (d, GIB)
        } else {
            (up.as_str(), 1)
        };
        digits
            .parse::<u64>()
            .ok()
            .and_then(|n| n.checked_mul(unit))
            .ok_or_else(|| format!("invalid number {s:?}"))
    }

    fn flag_value(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<u64, String> {
        let raw = args.next().ok_or_else(|| format!("{flag} needs a value"))?;
        parse_size(&raw).map_err(|e| format!("{flag}: {e}"))
    }

    fn parse_args() -> Result<Config, String> {
        let mut args = std::env::args().skip(1);
        let mut cfg = Config {
            path: String::new(),
            size: GIB,
            chunk: 4 * MIB as usize,
            depth: 8,
            runs: 5,
            destroy_data: false,
        };
        let mut have_path = false;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--size" => cfg.size = flag_value(&mut args, "--size")?,
                "--chunk" => cfg.chunk = flag_value(&mut args, "--chunk")? as usize,
                "--depth" => cfg.depth = flag_value(&mut args, "--depth")? as usize,
                "--runs" => cfg.runs = flag_value(&mut args, "--runs")?,
                "--destroy-data" => cfg.destroy_data = true,
                flag if flag.starts_with("--") => {
                    return Err(format!("unknown option {flag}\n{USAGE}"))
                }
                _ if have_path => return Err(format!("more than one target\n{USAGE}")),
                _ => {
                    cfg.path = arg;
                    have_path = true;
                }
            }
        }
        if !have_path {
            return Err(format!("missing target\n{USAGE}"));
        }
        if !cfg.chunk.is_power_of_two() || cfg.chunk < ALIGN {
            return Err(format!("--chunk must be a power of two, at least {ALIGN}"));
        }
        let chunk = cfg.chunk as u64;
        if cfg.size == 0 || cfg.size / chunk * chunk != cfg.size {
            return Err("--size must be a nonzero multiple of --chunk".into());
        }
        if !(1..=64).contains(&cfg.depth) {
            return Err("--depth must be in 1..=64".into());
        }
        if cfg.runs == 0 {
            return Err("--runs must be at least 1".into());
        }
        Ok(cfg)
    }

    #[derive(Clone, Copy)]
    enum Target {
        File,
        BlockDevice,
    }

    fn prepare(cfg: &Config) -> R<Target> {
        let is_block = std::fs::metadata(&cfg.path).is_ok_and(|m| m.file_type().is_block_device());
        if is_block {
            if !cfg.destroy_data {
                return Err(format!(
                    "{} is a block device and this overwrites it from offset 0; \
                     pass --destroy-data to proceed",
                    cfg.path
                )
                .into());
            }
            let device_size = std::fs::File::open(&cfg.path)?.seek(SeekFrom::End(0))?;
            if cfg.size > device_size {
                return Err(
                    format!("--size {} exceeds the device size {device_size}", cfg.size).into(),
                );
            }
        } else {
            let file = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(false)
                .open(&cfg.path)?;
            if file.metadata()?.len() < cfg.size {
                file.set_len(cfg.size)?;
            }
        }
        Ok(if is_block {
            Target::BlockDevice
        } else {
            Target::File
        })
    }

    #[allow(unsafe_code)]
    fn cpu_times() -> (Duration, Duration) {
        let tv = |t: libc::timeval| Duration::new(t.tv_sec as u64, t.tv_usec as u32 * 1000);
        // SAFETY: getrusage only writes into the zeroed rusage we pass, and RUSAGE_SELF is valid.
        let usage = unsafe {
            let mut usage: libc::rusage = std::mem::zeroed();
            libc::getrusage(libc::RUSAGE_SELF, &mut usage);
            usage
        };
        (tv(usage.ru_utime), tv(usage.ru_stime))
    }

    fn mix(x: u64) -> u64 {
        let z = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        let z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn fill(pool: &mut [Vec<u8>], id: u64) {
        for (k, buf) in pool.iter_mut().enumerate() {
            let seed = mix(id << 8 | k as u64);
            for (j, word) in buf.as_chunks_mut::<8>().0.iter_mut().enumerate() {
                *word = mix(seed.wrapping_add(j as u64)).to_le_bytes();
            }
        }
    }

    fn header(i: usize, id: u64) -> [u8; HEADER_LEN] {
        let mut h = [0; HEADER_LEN];
        h[..8].copy_from_slice(&(i as u64).to_le_bytes());
        h[8..].copy_from_slice(&id.to_le_bytes());
        h
    }

    fn view(buf: &[u8], chunk: usize, skew: usize) -> Range<usize> {
        let start = buf.as_ptr().align_offset(ALIGN) + skew;
        start..start + chunk
    }

    fn timed_write(
        cfg: &Config,
        engine: Engine,
        id: u64,
        pool: &mut [Vec<u8>],
    ) -> R<(Duration, Duration, Duration)> {
        enum Dev {
            Sync(Box<dyn RawDevice>),
            Uring(Box<IoUringDevice>),
        }
        let (chunk, n) = (cfg.chunk, (cfg.size / cfg.chunk as u64) as usize);
        let opts = OpenOptions::new().read(true).write(true).direct_io(true);
        let mut dev = match engine {
            Engine::Sync => Dev::Sync(open_device(&cfg.path, opts.block_size(chunk))?),
            Engine::SyncAligned => Dev::Sync(open_device(&cfg.path, opts.block_size(ALIGN))?),
            Engine::Uring { .. } => Dev::Uring(Box::new(IoUringDevice::open(
                &cfg.path,
                opts.block_size(ALIGN),
            )?)),
        };

        let (user0, sys0) = cpu_times();
        let start = Instant::now();
        for first in (0..n).step_by(engine.batch()) {
            let last = (first + engine.batch()).min(n);
            for i in first..last {
                let k = i % pool.len();
                let range = view(&pool[k], chunk, engine.skew());
                pool[k][range][..HEADER_LEN].copy_from_slice(&header(i, id));
            }
            match &mut dev {
                Dev::Sync(d) => {
                    let b = &pool[first % pool.len()];
                    d.write_all(&b[view(b, chunk, engine.skew())])?;
                }
                Dev::Uring(d) => {
                    d.write_batch((first..last).map(|i| {
                        let b = &pool[i % pool.len()];
                        ((i * chunk) as u64, &b[view(b, chunk, 0)])
                    }))?;
                }
            }
        }
        match &dev {
            Dev::Sync(d) => d.sync()?,
            Dev::Uring(d) => d.sync()?,
        }
        let wall = start.elapsed();
        let (user1, sys1) = cpu_times();
        Ok((wall, user1 - user0, sys1 - sys0))
    }

    fn verify(cfg: &Config, engine: Engine, id: u64, pool: &[Vec<u8>]) -> R<()> {
        let opts = OpenOptions::new()
            .read(true)
            .write(false)
            .direct_io(true)
            .block_size(ALIGN);
        let mut dev = open_device(&cfg.path, opts)?;
        let mut raw = vec![0u8; cfg.chunk + ALIGN];
        let span = view(&raw, cfg.chunk, 0);
        for i in 0..(cfg.size / cfg.chunk as u64) as usize {
            let got = &mut raw[span.clone()];
            let mut done = 0;
            while done < got.len() {
                let offset = (i * cfg.chunk + done) as u64;
                match dev.read_at(offset, &mut got[done..])? {
                    0 => return Err(format!("chunk {i}: unexpected end of device").into()),
                    n => done += n,
                }
            }
            let b = &pool[i % pool.len()];
            let want = &b[view(b, cfg.chunk, engine.skew())];
            if got[..HEADER_LEN] != header(i, id) {
                return Err(format!("chunk {i}: header mismatch").into());
            }
            if got[HEADER_LEN..] != want[HEADER_LEN..] {
                return Err(format!("chunk {i}: data mismatch").into());
            }
        }
        Ok(())
    }

    fn run_once(cfg: &Config, engine: Engine, id: u64, pool: &mut [Vec<u8>]) -> Run {
        fill(pool, id);
        let mut run = Run {
            id,
            engine,
            bytes: 0,
            wall: Duration::ZERO,
            user: Duration::ZERO,
            sys: Duration::ZERO,
            error: None,
        };
        match timed_write(cfg, engine, id, pool) {
            Ok((wall, user, sys)) => {
                (run.bytes, run.wall, run.user, run.sys) = (cfg.size, wall, user, sys);
                if let Err(e) = verify(cfg, engine, id, pool) {
                    run.error = Some(e.to_string());
                }
            }
            Err(e) => run.error = Some(e.to_string()),
        }
        run
    }

    fn sorted(ok: &[&Run], f: fn(&Run) -> f64) -> Vec<f64> {
        let mut v: Vec<f64> = ok.iter().map(|r| f(r)).collect();
        v.sort_by(f64::total_cmp);
        v
    }

    fn median(sorted: &[f64]) -> f64 {
        (sorted[(sorted.len() - 1) / 2] + sorted[sorted.len() / 2]) / 2.0
    }

    fn setup() -> R<(Config, Target)> {
        let cfg = parse_args()?;
        let target = prepare(&cfg)?;
        Ok((cfg, target))
    }

    pub fn main() -> ExitCode {
        let (cfg, target) = match setup() {
            Ok(setup) => setup,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::from(2);
            }
        };

        let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
            .map_or_else(|_| "unknown".into(), |s| s.trim().to_string());
        println!(
            "# target={} ({}) size={} chunk={} depth={} runs={} kernel={kernel}",
            cfg.path,
            match target {
                Target::File => "file",
                Target::BlockDevice => "block device",
            },
            cfg.size,
            cfg.chunk,
            cfg.depth,
            cfg.runs
        );
        println!("id\tengine\tbytes\twall_s\tmib_s\tuser_s\tsys_s\tcpu_pct\tstatus");

        let mut engines = vec![
            Engine::Sync,
            Engine::SyncAligned,
            Engine::Uring { depth: 1 },
        ];
        if cfg.depth > 1 {
            engines.push(Engine::Uring { depth: cfg.depth });
        }
        let mut pool = vec![vec![0u8; cfg.chunk + ALIGN + GLIBC_VEC_OFFSET]; cfg.depth];

        fill(&mut pool, 0);
        if let Err(e) = timed_write(&cfg, Engine::SyncAligned, 0, &mut pool) {
            eprintln!("error: warmup failed: {e}");
            return ExitCode::FAILURE;
        }

        let mut runs = Vec::new();
        let mut id = 0;
        for _ in 0..cfg.runs {
            for &engine in &engines {
                id += 1;
                let r = run_once(&cfg, engine, id, &mut pool);
                println!(
                    "{}\t{}\t{}\t{:.3}\t{:.1}\t{:.3}\t{:.3}\t{:.1}\t{}",
                    r.id,
                    r.engine.name(),
                    r.bytes,
                    r.wall.as_secs_f64(),
                    r.mib_s(),
                    r.user.as_secs_f64(),
                    r.sys.as_secs_f64(),
                    r.cpu_pct(),
                    r.error.as_deref().unwrap_or("ok")
                );
                runs.push(r);
            }
        }

        println!(
            "\n{:<14}{:>8}{:>14}{:>10}{:>10}{:>13}{:>12}",
            "engine", "ok/runs", "median MiB/s", "min", "max", "cpu ms/GiB", "cpu % wall"
        );
        for &engine in &engines {
            let all: Vec<&Run> = runs.iter().filter(|r| r.engine == engine).collect();
            let ok: Vec<&Run> = all.iter().copied().filter(|r| r.error.is_none()).collect();
            let counts = format!("{}/{}", ok.len(), all.len());
            if ok.is_empty() {
                println!("{:<14}{counts:>8}", engine.name());
                continue;
            }
            let rate = sorted(&ok, Run::mib_s);
            println!(
                "{:<14}{counts:>8}{:>14.1}{:>10.1}{:>10.1}{:>13.0}{:>12.1}",
                engine.name(),
                median(&rate),
                rate[0],
                rate[rate.len() - 1],
                median(&sorted(&ok, Run::cpu_ms_per_gib)),
                median(&sorted(&ok, Run::cpu_pct))
            );
        }

        if runs.iter().any(|r| r.error.is_some()) {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        }
    }
}

#[cfg(target_os = "linux")]
use bench::main;

#[cfg(not(target_os = "linux"))]
fn main() {
    println!("uring_bench is Linux-only: it measures io_uring and O_DIRECT writes.");
}
