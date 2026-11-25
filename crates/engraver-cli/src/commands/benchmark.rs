//! Benchmark command - tests write speed of a drive
//!
//! This command allows users to benchmark the write speed of a storage device
//! before committing to a potentially long write operation. It helps identify
//! slow drives or USB connections.
//!
//! **Warning:** This is a destructive operation that will overwrite data on the target device.

use anyhow::{bail, Context, Result};
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use engraver_core::{
    format_size, is_power_of_two, parse_block_sizes, parse_size, BenchmarkConfig, BenchmarkError,
    BenchmarkProgress, BenchmarkResult, BenchmarkRunner, BlockSizeTestResult, DataPattern,
};
use engraver_detect::list_drives;
use engraver_platform::{has_elevated_privileges, open_device, unmount_device, OpenOptions};

use super::{cancelled, confirm};

#[derive(Debug, PartialEq, Eq)]
pub enum TestSize {
    Exact(String),
    Default(String),
}

/// Arguments for the benchmark command
pub struct BenchmarkArgs {
    /// Target device path
    pub target: String,
    pub test_size: TestSize,
    /// Block size (e.g., "4M")
    pub block_size: String,
    /// Data pattern: zeros, random, sequential
    pub pattern: String,
    /// Number of passes
    pub passes: u32,
    /// Output in JSON format
    pub json: bool,
    /// Skip confirmation prompt
    pub skip_confirm: bool,
    /// Hide status output (`-q`, `--silent`)
    pub silent: bool,
    /// Test multiple block sizes
    pub test_block_sizes: Option<String>,
    /// Cancellation flag
    pub cancel_flag: Arc<AtomicBool>,
}

impl BenchmarkArgs {
    /// Whether to withhold the human-readable status lines.
    ///
    /// `--json` shares stdout with these lines, so it hides them like `--silent`.
    fn hides_status_lines(&self) -> bool {
        self.silent || self.json
    }
}

/// Execute the benchmark command
pub fn execute(args: BenchmarkArgs) -> Result<()> {
    let silent = args.hides_status_lines();

    // Step 0: Validate arguments (before any I/O)
    validate_args(&args)?;

    // Step 1: Check for elevated privileges
    if !has_elevated_privileges() {
        #[cfg(unix)]
        bail!(
            "Root privileges required.\n\
             Try running with: sudo engraver benchmark ..."
        );

        #[cfg(windows)]
        bail!(
            "Administrator privileges required.\n\
             Right-click and select 'Run as administrator'."
        );

        #[cfg(not(any(unix, windows)))]
        bail!("Elevated privileges required for raw device access.");
    }

    // Step 2: Find and validate target device
    let drives = list_drives().context("Failed to list drives")?;
    let target_drive = find_target_drive(&drives, &args.target)?;

    // Refuse to benchmark system drives
    if target_drive.is_system {
        bail!(
            "Refusing to benchmark system drive: {}\n\
             This appears to be your system drive and benchmarking would destroy your OS.",
            target_drive.path
        );
    }

    // Step 3: Parse configuration
    let pattern = DataPattern::from_str(&args.pattern).map_err(|e| anyhow::anyhow!("{}", e))?;

    let block_size = parse_size(&args.block_size).map_err(|e| anyhow::anyhow!("{}", e))?;

    // Determine if we're doing multi-block-size test
    let is_multi_block = args.test_block_sizes.is_some();
    let block_sizes: Vec<u64> = if let Some(ref sizes_str) = args.test_block_sizes {
        parse_block_sizes(sizes_str).map_err(|e| anyhow::anyhow!("{}", e))?
    } else {
        vec![]
    };

    let test_size = resolve_test_size(
        &args,
        if is_multi_block {
            &block_sizes
        } else {
            std::slice::from_ref(&block_size)
        },
    )?;

    // Step 4: Display configuration
    display_benchmark_info(
        target_drive,
        test_size,
        block_size,
        &pattern,
        args.passes,
        is_multi_block,
        &block_sizes,
        silent,
    );

    // Step 5: Safety confirmation
    if !args.skip_confirm && !confirm_benchmark(target_drive)? {
        println_if!(silent, "{}", style("Aborted.").yellow());
        return Ok(());
    }

    // Step 6: Unmount device
    println_if!(silent, "\n{} Unmounting device...", style("▶").cyan());
    if let Err(e) = unmount_device(&target_drive.path) {
        println_if!(
            silent,
            "  {} Could not unmount: {} (continuing anyway)",
            style("⚠").yellow(),
            e
        );
    } else {
        println_if!(silent, "  {} Device unmounted", style("✓").green());
    }

    // Step 7: Open device for writing
    println_if!(silent, "{} Opening device...", style("▶").cyan());
    let mut device = open_device(
        &target_drive.path,
        OpenOptions::new().write(true).direct_io(true),
    )
    .context("Failed to open device for writing")?;
    println_if!(silent, "  {} Device opened", style("✓").green());

    // Step 8: Run benchmark
    let result = if is_multi_block {
        run_multi_block_benchmark(
            &mut *device,
            &target_drive.path,
            test_size,
            &block_sizes,
            pattern,
            args.json,
            silent,
            args.cancel_flag,
        )
    } else {
        run_single_benchmark(
            &mut *device,
            &target_drive.path,
            test_size,
            block_size,
            pattern,
            args.passes,
            args.json,
            silent,
            args.cancel_flag,
        )
    };

    // Step 9: Sync device to flush any pending writes
    if let Err(e) = device.sync() {
        tracing::debug!("Sync after benchmark: {}", e);
    }

    if result.is_ok() {
        println_if!(
            silent,
            "{}",
            style("You can safely remove the drive.").green()
        );
    }

    result
}

fn resolve_test_size(args: &BenchmarkArgs, block_sizes: &[u64]) -> Result<u64> {
    match &args.test_size {
        TestSize::Exact(size_str) => Ok(parse_size(size_str)?),
        TestSize::Default(size_str) => {
            let base = parse_size(size_str)?;
            Ok(BenchmarkConfig::default_test_size(base, block_sizes))
        }
    }
}

/// Validate command arguments before any I/O
fn validate_args(args: &BenchmarkArgs) -> Result<()> {
    // Check mutual exclusivity
    if matches!(args.test_size, TestSize::Exact(_)) && args.test_block_sizes.is_some() {
        bail!(
            "Cannot use both --size and --test-block-sizes options.\n\
             Use --size for a single block size benchmark, or --test-block-sizes to test multiple block sizes."
        );
    }

    // Validate test size if provided
    if let TestSize::Exact(size_str) = &args.test_size {
        let size = parse_size(size_str).map_err(|e| anyhow::anyhow!("{}", e))?;
        if !is_power_of_two(size) {
            bail!("Test size must be a power of 2: {}", size_str);
        }
    }
    if let TestSize::Default(size_str) = &args.test_size {
        parse_size(size_str).map_err(|e| anyhow::anyhow!("{}", e))?;
    }

    // Validate block size
    let block_size = parse_size(&args.block_size).map_err(|e| anyhow::anyhow!("{}", e))?;
    if !is_power_of_two(block_size) {
        bail!("Block size must be a power of 2: {}", args.block_size);
    }
    if block_size > 64 * 1024 * 1024 {
        bail!("Block size cannot exceed 64 MB: {}", args.block_size);
    }
    if block_size < 4 * 1024 {
        bail!("Block size cannot be less than 4 KB: {}", args.block_size);
    }

    if let TestSize::Exact(size_str) = &args.test_size {
        let size = parse_size(size_str).map_err(|e| anyhow::anyhow!("{}", e))?;
        BenchmarkConfig::new(size, block_size)
            .validate()
            .map_err(|e| anyhow::anyhow!("{}", e))?;
    }

    // Validate test block sizes if provided
    if let Some(ref sizes_str) = args.test_block_sizes {
        let sizes = parse_block_sizes(sizes_str).map_err(|e| anyhow::anyhow!("{}", e))?;
        for size in &sizes {
            if !is_power_of_two(*size) {
                bail!("All block sizes must be powers of 2");
            }
            if *size > 64 * 1024 * 1024 {
                bail!("Block sizes cannot exceed 64 MB");
            }
            if *size < 4 * 1024 {
                bail!("Block sizes cannot be less than 4 KB");
            }
        }
    }

    // Validate pattern
    DataPattern::from_str(&args.pattern).map_err(|e| anyhow::anyhow!("{}", e))?;

    Ok(())
}

/// Find the target drive from the list of available drives
fn find_target_drive<'a>(
    drives: &'a [engraver_detect::Drive],
    target: &str,
) -> Result<&'a engraver_detect::Drive> {
    // Normalize target path
    let normalized_target = if target.starts_with("/dev/") || target.starts_with("\\\\.\\") {
        target.to_string()
    } else {
        #[cfg(unix)]
        {
            format!("/dev/{}", target)
        }
        #[cfg(windows)]
        {
            format!("\\\\.\\{}", target)
        }
        #[cfg(not(any(unix, windows)))]
        {
            target.to_string()
        }
    };

    drives
        .iter()
        .find(|d| d.path == normalized_target || d.path == target)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Device not found: {}\n\
                 Run 'engraver list' to see available drives.",
                target
            )
        })
}

/// Display benchmark configuration
fn display_benchmark_info(
    drive: &engraver_detect::Drive,
    test_size: u64,
    block_size: u64,
    pattern: &DataPattern,
    passes: u32,
    is_multi_block: bool,
    block_sizes: &[u64],
    silent: bool,
) {
    println_if!(silent);
    println_if!(
        silent,
        "{} {} ({})",
        style("Benchmark:").bold(),
        style(&drive.path).cyan(),
        drive.model.as_deref().unwrap_or(&drive.name)
    );

    println_if!(silent, "  Size: {}", format_size(drive.size));

    if let Some(ref speed) = drive.usb_speed {
        println_if!(silent, "  USB: {}", speed);
    }

    println_if!(silent);
    println_if!(silent, "{}", style("Configuration:").bold());

    if is_multi_block {
        let sizes_display: Vec<String> = block_sizes.iter().map(|s| format_size(*s)).collect();
        println_if!(silent, "  Block sizes: {}", sizes_display.join(", "));
        println_if!(
            silent,
            "  Test size: {} (per block size)",
            format_size(test_size)
        );
    } else {
        let per_pass = if passes > 1 { " per pass" } else { "" };
        println_if!(
            silent,
            "  Test size: {}{}",
            format_size(test_size),
            per_pass
        );
        println_if!(silent, "  Block size: {}", format_size(block_size));
    }

    let pattern_str = match pattern {
        DataPattern::Zeros => "zeros",
        DataPattern::Random => "random",
        DataPattern::Sequential => "sequential",
        _ => "unknown",
    };
    println_if!(silent, "  Pattern: {}", pattern_str);

    if !is_multi_block {
        println_if!(silent, "  Passes: {}", passes);
    }
}

/// Confirm benchmark with user
fn confirm_benchmark(drive: &engraver_detect::Drive) -> Result<bool> {
    eprintln!();
    eprintln!(
        "{}",
        style("╔══════════════════════════════════════════════════════════════╗")
            .red()
            .bold()
    );
    eprintln!(
        "{}",
        style("║                     DESTRUCTIVE OPERATION                     ║")
            .red()
            .bold()
    );
    eprintln!(
        "{}",
        style("║  This benchmark will OVERWRITE data on the target device!    ║")
            .red()
            .bold()
    );
    eprintln!(
        "{}",
        style("║  ALL DATA ON THE TARGET DEVICE WILL BE PERMANENTLY LOST!     ║")
            .red()
            .bold()
    );
    eprintln!(
        "{}",
        style("╚══════════════════════════════════════════════════════════════╝")
            .red()
            .bold()
    );
    eprintln!();

    let size_str = format_size(drive.size);

    let confirm_text = format!(
        "Benchmark {} ({})? This WILL DESTROY ALL DATA on this drive",
        drive.path, size_str
    );

    confirm(&confirm_text, false)
}

/// Get progress bar style based on percentage
fn get_progress_style(percentage: u8) -> ProgressStyle {
    let (color, filled_char) = match percentage {
        0..=49 => ("red", '█'),
        50..=74 => ("yellow", '█'),
        75..=99 => ("green", '█'),
        100 => ("blue", '█'),
        _ => ("white", '█'),
    };

    ProgressStyle::default_bar()
        .template(&format!(
            "  [{{bar:40.{color}/{color}}}] {{pos}}/{{len}} {{msg}}"
        ))
        .unwrap()
        .progress_chars(&format!("{}░", filled_char))
}

/// Run single block size benchmark
fn run_single_benchmark<W>(
    device: &mut W,
    device_path: &str,
    test_size: u64,
    block_size: u64,
    pattern: DataPattern,
    passes: u32,
    json: bool,
    silent: bool,
    cancel_flag: Arc<AtomicBool>,
) -> Result<()>
where
    W: std::io::Write + std::io::Seek + ?Sized,
{
    let config = BenchmarkConfig {
        test_size,
        block_size,
        pattern,
        passes,
    };

    let total_bytes = test_size * passes as u64;

    println_if!(silent, "\n{} Benchmarking...", style("▶").cyan());

    // Create progress bar
    let pb = if silent {
        ProgressBar::hidden()
    } else {
        let pb = ProgressBar::new(total_bytes);
        pb.set_style(get_progress_style(0));
        pb
    };

    // Share the program-wide cancel flag directly with the benchmark
    // runner. Both layers use `true = cancel` so a single store(true)
    // from the Ctrl+C handler reaches the run loop without a polling
    // thread to bridge two separate atomics.
    let runner = BenchmarkRunner::new(config).with_cancel_flag(cancel_flag);

    let pb_clone = pb.clone();
    let result = runner.run(
        device,
        device_path,
        Some(move |progress: &BenchmarkProgress| {
            let pct = progress.percentage();
            pb_clone.set_style(get_progress_style(pct));
            pb_clone.set_position(progress.bytes_written);
            pb_clone.set_message(format!(
                "{} {}",
                progress.speed_display(),
                format_eta(progress)
            ));
        }),
    );

    pb.finish_and_clear();

    match result {
        Ok(result) => {
            if json {
                output_json(&result)?;
            } else {
                output_human_readable(&result, silent);
            }
            Ok(())
        }
        Err(BenchmarkError::Cancelled) => Err(cancelled("Benchmark cancelled")),
        Err(e) => Err(anyhow::anyhow!("Benchmark failed: {}", e)),
    }
}

/// Run multi-block-size benchmark
fn run_multi_block_benchmark<W>(
    device: &mut W,
    device_path: &str,
    test_size: u64,
    block_sizes: &[u64],
    pattern: DataPattern,
    json: bool,
    silent: bool,
    cancel_flag: Arc<AtomicBool>,
) -> Result<()>
where
    W: std::io::Write + std::io::Seek + ?Sized,
{
    let total_tests = block_sizes.len();

    println_if!(
        silent,
        "\n{} Running {} block size tests...",
        style("▶").cyan(),
        total_tests
    );

    let mut results: Vec<BlockSizeTestResult> = Vec::new();

    for (idx, &block_size) in block_sizes.iter().enumerate() {
        if cancel_flag.load(Ordering::Relaxed) {
            return Err(cancelled("Benchmark cancelled"));
        }

        println_if!(
            silent,
            "\n  {} Testing block size {} ({}/{})",
            style("▶").cyan(),
            format_size(block_size),
            idx + 1,
            total_tests
        );

        let test_config = BenchmarkConfig {
            test_size,
            block_size,
            pattern,
            passes: 1,
        };

        // Seek to beginning
        device.seek(std::io::SeekFrom::Start(0))?;

        let pb = if silent {
            ProgressBar::hidden()
        } else {
            let pb = ProgressBar::new(test_size);
            pb.set_style(get_progress_style(0));
            pb
        };

        let runner = BenchmarkRunner::new(test_config).with_cancel_flag(Arc::clone(&cancel_flag));

        let pb_clone = pb.clone();
        let result = runner.run(
            &mut *device,
            device_path,
            Some(move |progress: &BenchmarkProgress| {
                let pct = progress.percentage();
                pb_clone.set_style(get_progress_style(pct));
                pb_clone.set_position(progress.bytes_written);
                pb_clone.set_message(progress.speed_display());
            }),
        );

        pb.finish_and_clear();

        match result {
            Ok(bench_result) => {
                let speed = bench_result.summary.average_speed_bps;
                println_if!(
                    silent,
                    "    {} {}: {}",
                    style("✓").green(),
                    format_size(block_size),
                    engraver_core::benchmark::format_speed(speed)
                );

                results.push(BlockSizeTestResult {
                    block_size,
                    block_size_display: format_size(block_size),
                    bytes_written: bench_result.summary.total_bytes_written,
                    average_speed_bps: speed,
                    speed_display: engraver_core::benchmark::format_speed(speed),
                });
            }
            Err(BenchmarkError::Cancelled) => return Err(cancelled("Benchmark cancelled")),
            Err(e) => {
                println_if!(
                    silent,
                    "    {} {}: Failed - {}",
                    style("✗").red(),
                    format_size(block_size),
                    e
                );
            }
        }
    }

    if json {
        output_multi_block_json(&results)?;
    } else {
        output_multi_block_human(&results, silent);
    }

    Ok(())
}

/// Format ETA from progress
fn format_eta(progress: &BenchmarkProgress) -> String {
    if progress.current_speed_bps == 0 {
        return String::new();
    }

    let remaining_bytes = progress.total_bytes.saturating_sub(progress.bytes_written);
    let eta_secs = remaining_bytes / progress.current_speed_bps;

    if eta_secs > 60 {
        format!("ETA: {}m {}s", eta_secs / 60, eta_secs % 60)
    } else {
        format!("ETA: {}s", eta_secs)
    }
}

/// Output results as JSON
fn output_json(result: &BenchmarkResult) -> Result<()> {
    let json = serde_json::to_string_pretty(result)?;
    println!("{}", json);
    Ok(())
}

/// Output multi-block results as JSON
fn output_multi_block_json(results: &[BlockSizeTestResult]) -> Result<()> {
    let json = serde_json::to_string_pretty(results)?;
    println!("{}", json);
    Ok(())
}

/// Output results in human-readable format
fn output_human_readable(result: &BenchmarkResult, silent: bool) {
    println_if!(silent);
    println_if!(silent, "{}", style("Results:").bold().green());
    println_if!(
        silent,
        "  Average Speed:  {}",
        style(engraver_core::benchmark::format_speed(
            result.summary.average_speed_bps
        ))
        .cyan()
        .bold()
    );
    println_if!(
        silent,
        "  Minimum Speed:  {}",
        engraver_core::benchmark::format_speed(result.summary.min_speed_bps)
    );
    println_if!(
        silent,
        "  Maximum Speed:  {}",
        engraver_core::benchmark::format_speed(result.summary.max_speed_bps)
    );
    println_if!(
        silent,
        "  Total Time:     {}",
        engraver_core::benchmark::format_duration(result.summary.total_elapsed)
    );
    let passes = result.passes.len();
    if passes > 1 {
        println_if!(
            silent,
            "  Data Written:   {} passes x {}",
            passes,
            format_size(result.test_size)
        );
    } else {
        println_if!(
            silent,
            "  Data Written:   {}",
            format_size(result.summary.total_bytes_written)
        );
    }
    println_if!(silent);
    println_if!(silent, "{} Benchmark complete!", style("✓").green().bold());
}

/// Output multi-block results in human-readable format
fn output_multi_block_human(results: &[BlockSizeTestResult], silent: bool) {
    println_if!(silent);
    println_if!(
        silent,
        "{}",
        style("Block Size Performance Comparison:").bold().green()
    );
    println_if!(silent, "  ┌──────────────┬────────────────┐");
    println_if!(silent, "  │ Block Size   │ Avg Speed      │");
    println_if!(silent, "  ├──────────────┼────────────────┤");

    // Find optimal (fastest) block size
    let optimal = results.iter().max_by_key(|r| r.average_speed_bps);

    for result in results {
        let is_optimal = optimal.is_some_and(|o| o.block_size == result.block_size);
        let marker = if is_optimal { " ← Optimal" } else { "" };

        println_if!(
            silent,
            "  │ {:12} │ {:14} │{}",
            result.block_size_display,
            result.speed_display,
            style(marker).green()
        );
    }

    println_if!(silent, "  └──────────────┴────────────────┘");

    if let Some(first) = results.first() {
        println_if!(
            silent,
            "  Data written per block size: {}",
            format_size(first.bytes_written)
        );
    }

    if let Some(opt) = optimal {
        println_if!(silent);
        println_if!(
            silent,
            "{} Use {} block size for best performance",
            style("Recommendation:").bold(),
            style(&opt.block_size_display).cyan().bold()
        );
    }

    println_if!(silent);
    println_if!(silent, "{} Benchmark complete!", style("✓").green().bold());
}

#[cfg(test)]
mod tests {
    use super::*;
    use engraver_detect::{Drive, DriveType, UsbSpeed};
    use std::time::Duration;

    // -------------------------------------------------------------------------
    // BenchmarkArgs tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_benchmark_args_creation() {
        let args = BenchmarkArgs {
            target: "/dev/sdb".to_string(),
            test_size: TestSize::Exact("256M".to_string()),
            block_size: "4M".to_string(),
            pattern: "zeros".to_string(),
            passes: 1,
            json: false,
            skip_confirm: true,
            silent: false,
            test_block_sizes: None,
            cancel_flag: Arc::new(AtomicBool::new(true)),
        };

        assert_eq!(args.target, "/dev/sdb");
        assert_eq!(args.test_size, TestSize::Exact("256M".to_string()));
        assert_eq!(args.block_size, "4M");
        assert_eq!(args.pattern, "zeros");
        assert_eq!(args.passes, 1);
        assert!(!args.json);
        assert!(args.skip_confirm);
    }

    #[test]
    fn test_json_and_silent_hide_status_lines() {
        let args = |json, silent| BenchmarkArgs {
            target: "/dev/sdb".to_string(),
            test_size: TestSize::Default("256M".to_string()),
            block_size: "4M".to_string(),
            pattern: "zeros".to_string(),
            passes: 1,
            json,
            skip_confirm: true,
            silent,
            test_block_sizes: None,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        };

        assert!(args(true, false).hides_status_lines());
        assert!(args(false, true).hides_status_lines());
        assert!(args(true, true).hides_status_lines());
        assert!(!args(false, false).hides_status_lines());
    }

    // -------------------------------------------------------------------------
    // validate_args tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_validate_args_valid() {
        let args = BenchmarkArgs {
            target: "/dev/sdb".to_string(),
            test_size: TestSize::Exact("256M".to_string()),
            block_size: "4M".to_string(),
            pattern: "zeros".to_string(),
            passes: 1,
            json: false,
            skip_confirm: true,
            silent: false,
            test_block_sizes: None,
            cancel_flag: Arc::new(AtomicBool::new(true)),
        };

        assert!(validate_args(&args).is_ok());
    }

    #[test]
    fn test_validate_args_mutual_exclusion() {
        let args = BenchmarkArgs {
            target: "/dev/sdb".to_string(),
            test_size: TestSize::Exact("256M".to_string()),
            block_size: "4M".to_string(),
            pattern: "zeros".to_string(),
            passes: 1,
            json: false,
            skip_confirm: true,
            silent: false,
            test_block_sizes: Some("4K,1M,4M".to_string()),
            cancel_flag: Arc::new(AtomicBool::new(true)),
        };

        let result = validate_args(&args);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Cannot use both"));
    }

    #[test]
    fn test_validate_args_block_size_too_large() {
        let args = BenchmarkArgs {
            target: "/dev/sdb".to_string(),
            test_size: TestSize::Default("256M".to_string()),
            block_size: "128M".to_string(),
            pattern: "zeros".to_string(),
            passes: 1,
            json: false,
            skip_confirm: true,
            silent: false,
            test_block_sizes: None,
            cancel_flag: Arc::new(AtomicBool::new(true)),
        };

        let result = validate_args(&args);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("64 MB"));
    }

    #[test]
    fn test_validate_args_block_size_too_small() {
        let args = BenchmarkArgs {
            target: "/dev/sdb".to_string(),
            test_size: TestSize::Default("256M".to_string()),
            block_size: "1K".to_string(),
            pattern: "zeros".to_string(),
            passes: 1,
            json: false,
            skip_confirm: true,
            silent: false,
            test_block_sizes: None,
            cancel_flag: Arc::new(AtomicBool::new(true)),
        };

        let result = validate_args(&args);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("4 KB"));
    }

    #[test]
    fn test_validate_args_invalid_pattern() {
        let args = BenchmarkArgs {
            target: "/dev/sdb".to_string(),
            test_size: TestSize::Default("256M".to_string()),
            block_size: "4M".to_string(),
            pattern: "invalid_pattern".to_string(),
            passes: 1,
            json: false,
            skip_confirm: true,
            silent: false,
            test_block_sizes: None,
            cancel_flag: Arc::new(AtomicBool::new(true)),
        };

        let result = validate_args(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_args_valid_patterns() {
        for pattern in &["zeros", "random", "sequential", "seq", "zero"] {
            let args = BenchmarkArgs {
                target: "/dev/sdb".to_string(),
                test_size: TestSize::Default("256M".to_string()),
                block_size: "4M".to_string(),
                pattern: pattern.to_string(),
                passes: 1,
                json: false,
                skip_confirm: true,
                silent: false,
                test_block_sizes: None,
                cancel_flag: Arc::new(AtomicBool::new(true)),
            };

            assert!(
                validate_args(&args).is_ok(),
                "Pattern '{}' should be valid",
                pattern
            );
        }
    }

    #[test]
    fn test_validate_args_test_block_sizes() {
        let args = BenchmarkArgs {
            target: "/dev/sdb".to_string(),
            test_size: TestSize::Default("256M".to_string()),
            block_size: "4M".to_string(),
            pattern: "zeros".to_string(),
            passes: 1,
            json: false,
            skip_confirm: true,
            silent: false,
            test_block_sizes: Some("4K,64K,1M,4M,16M".to_string()),
            cancel_flag: Arc::new(AtomicBool::new(true)),
        };

        assert!(validate_args(&args).is_ok());
    }

    const MIB: u64 = 1024 * 1024;

    fn size_args(test_size: TestSize, block_size: &str) -> BenchmarkArgs {
        BenchmarkArgs {
            target: "/dev/sdb".to_string(),
            test_size,
            block_size: block_size.to_string(),
            pattern: "zeros".to_string(),
            passes: 1,
            json: false,
            skip_confirm: true,
            silent: false,
            test_block_sizes: None,
            cancel_flag: Arc::new(AtomicBool::new(true)),
        }
    }

    #[test]
    fn test_resolve_test_size_writes_a_given_size_as_is() {
        let args = size_args(TestSize::Exact("64M".to_string()), "32M");

        assert_eq!(resolve_test_size(&args, &[32 * MIB]).unwrap(), 64 * MIB);
    }

    #[test]
    fn test_resolve_test_size_default_is_ten_blocks_when_the_default_is_fewer() {
        let args = size_args(TestSize::Default("256M".to_string()), "32M");

        assert_eq!(resolve_test_size(&args, &[32 * MIB]).unwrap(), 320 * MIB);
        assert_eq!(
            resolve_test_size(&args, &[4 * 1024, 32 * MIB]).unwrap(),
            320 * MIB
        );
    }

    #[test]
    fn test_resolve_test_size_default_is_kept_when_it_holds_ten_blocks() {
        let args = size_args(TestSize::Default("256M".to_string()), "4M");

        assert_eq!(resolve_test_size(&args, &[4 * MIB]).unwrap(), 256 * MIB);
    }

    #[test]
    fn test_resolve_test_size_uses_the_configured_default() {
        let args = size_args(TestSize::Default("512M".to_string()), "32M");

        assert_eq!(resolve_test_size(&args, &[32 * MIB]).unwrap(), 512 * MIB);
    }

    #[test]
    fn test_validate_args_refuses_a_size_smaller_than_one_block() {
        let args = size_args(TestSize::Exact("4M".to_string()), "64M");

        let err = validate_args(&args).unwrap_err().to_string();

        assert!(err.contains("smaller than the block size"), "{err}");
        assert!(err.contains("4 MB") && err.contains("64 MB"), "{err}");
    }

    #[test]
    fn test_validate_args_accepts_a_size_of_exactly_one_block() {
        let args = size_args(TestSize::Exact("64M".to_string()), "64M");

        assert!(validate_args(&args).is_ok());
    }

    #[test]
    fn test_validate_args_rejects_a_default_size_that_is_not_a_power_of_2() {
        let args = size_args(TestSize::Default("100M".to_string()), "4M");

        assert!(validate_args(&args).is_err());
    }

    // -------------------------------------------------------------------------
    // format_eta tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_format_eta_zero_speed() {
        let progress = BenchmarkProgress {
            bytes_written: 100,
            total_bytes: 1000,
            current_pass: 1,
            total_passes: 1,
            current_speed_bps: 0,
            elapsed: Duration::from_secs(1),
        };

        assert_eq!(format_eta(&progress), "");
    }

    #[test]
    fn test_format_eta_seconds() {
        let progress = BenchmarkProgress {
            bytes_written: 500,
            total_bytes: 1000,
            current_pass: 1,
            total_passes: 1,
            current_speed_bps: 100, // 100 B/s -> 500 remaining / 100 = 5s
            elapsed: Duration::from_secs(5),
        };

        assert_eq!(format_eta(&progress), "ETA: 5s");
    }

    #[test]
    fn test_format_eta_minutes() {
        let progress = BenchmarkProgress {
            bytes_written: 0,
            total_bytes: 7500,
            current_pass: 1,
            total_passes: 1,
            current_speed_bps: 100, // 7500 / 100 = 75s = 1m 15s
            elapsed: Duration::from_secs(0),
        };

        assert_eq!(format_eta(&progress), "ETA: 1m 15s");
    }

    #[test]
    fn test_format_eta_complete() {
        let progress = BenchmarkProgress {
            bytes_written: 1000,
            total_bytes: 1000,
            current_pass: 1,
            total_passes: 1,
            current_speed_bps: 100,
            elapsed: Duration::from_secs(10),
        };

        assert_eq!(format_eta(&progress), "ETA: 0s");
    }

    // -------------------------------------------------------------------------
    // get_progress_style tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_get_progress_style_stages() {
        // Just verify each stage returns a valid style without panicking
        let _ = get_progress_style(0);
        let _ = get_progress_style(25);
        let _ = get_progress_style(49);
        let _ = get_progress_style(50);
        let _ = get_progress_style(74);
        let _ = get_progress_style(75);
        let _ = get_progress_style(99);
        let _ = get_progress_style(100);
    }

    // -------------------------------------------------------------------------
    // find_target_drive tests
    // -------------------------------------------------------------------------

    fn create_test_drive(path: &str, is_system: bool) -> Drive {
        Drive {
            path: path.to_string(),
            raw_path: path.to_string(),
            name: path.split('/').next_back().unwrap_or("drive").to_string(),
            size: 16 * 1024 * 1024 * 1024,
            removable: !is_system,
            drive_type: DriveType::Usb,
            vendor: Some("Test".to_string()),
            model: Some("Drive".to_string()),
            serial: None,
            partitions: vec![],
            mount_points: vec![],
            is_system,
            system_reason: if is_system {
                Some("Test system drive".to_string())
            } else {
                None
            },
            usb_speed: Some(UsbSpeed::SuperSpeed),
        }
    }

    #[test]
    fn test_find_target_drive_exact_match() {
        let drives = vec![
            create_test_drive("/dev/sda", true),
            create_test_drive("/dev/sdb", false),
        ];

        let result = find_target_drive(&drives, "/dev/sdb");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().path, "/dev/sdb");
    }

    #[test]
    fn test_find_target_drive_not_found() {
        let drives = vec![create_test_drive("/dev/sda", true)];

        let result = find_target_drive(&drives, "/dev/sdb");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"));
    }

    #[test]
    fn test_find_target_drive_short_form() {
        let _drives = [create_test_drive("/dev/sdb", false)];

        // On Linux, should normalize "sdb" to "/dev/sdb"
        #[cfg(target_os = "linux")]
        {
            let result = find_target_drive(&_drives, "sdb");
            assert!(result.is_ok());
        }
    }

    // -------------------------------------------------------------------------
    // Cancellation tests
    // -------------------------------------------------------------------------

    const TEST_SIZE: u64 = 64 * 1024;
    const TEST_BLOCK: u64 = 4 * 1024;

    fn scratch_device() -> std::io::Cursor<Vec<u8>> {
        std::io::Cursor::new(vec![0u8; 1024 * 1024])
    }

    #[test]
    fn test_single_benchmark_with_pending_cancel_returns_a_cancelled_error() {
        let mut device = scratch_device();

        let err = run_single_benchmark(
            &mut device,
            "/dev/test",
            TEST_SIZE,
            TEST_BLOCK,
            DataPattern::Zeros,
            1,
            true,
            true,
            Arc::new(AtomicBool::new(true)),
        )
        .expect_err("a pending cancel must stop the benchmark");

        crate::commands::assert_cancelled(&err);
    }

    #[test]
    fn test_single_benchmark_without_cancel_succeeds() {
        let mut device = scratch_device();

        run_single_benchmark(
            &mut device,
            "/dev/test",
            TEST_SIZE,
            TEST_BLOCK,
            DataPattern::Zeros,
            1,
            true,
            true,
            Arc::new(AtomicBool::new(false)),
        )
        .expect("an uncancelled benchmark must succeed");
    }

    #[test]
    fn test_multi_block_benchmark_with_pending_cancel_returns_a_cancelled_error() {
        let mut device = scratch_device();

        let err = run_multi_block_benchmark(
            &mut device,
            "/dev/test",
            TEST_SIZE,
            &[TEST_BLOCK, 2 * TEST_BLOCK],
            DataPattern::Zeros,
            true,
            true,
            Arc::new(AtomicBool::new(true)),
        )
        .expect_err("a pending cancel must stop the benchmark");

        crate::commands::assert_cancelled(&err);
    }

    #[derive(Default)]
    struct ExtentSink {
        position: u64,
        extent: u64,
    }

    impl std::io::Write for ExtentSink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.position += buf.len() as u64;
            self.extent = self.extent.max(self.position);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl std::io::Seek for ExtentSink {
        fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
            match pos {
                std::io::SeekFrom::Start(offset) => self.position = offset,
                _ => return Err(std::io::ErrorKind::Unsupported.into()),
            }
            Ok(self.position)
        }
    }

    #[test]
    fn test_single_benchmark_writes_exactly_the_given_size() {
        let mut sink = ExtentSink::default();

        run_single_benchmark(
            &mut sink,
            "/dev/test",
            4 * TEST_BLOCK,
            TEST_BLOCK,
            DataPattern::Zeros,
            1,
            true,
            true,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();

        assert_eq!(sink.extent, 4 * TEST_BLOCK);
    }

    #[test]
    fn test_multi_block_benchmark_writes_the_given_size_for_every_block_size() {
        let mut sink = ExtentSink::default();
        let size = 10 * 32 * 1024 * 1024;

        run_multi_block_benchmark(
            &mut sink,
            "/dev/test",
            size,
            &[TEST_BLOCK, 32 * 1024 * 1024],
            DataPattern::Zeros,
            true,
            true,
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();

        assert_eq!(sink.extent, size);
    }

    #[test]
    fn test_multi_block_benchmark_without_cancel_succeeds() {
        let mut device = scratch_device();

        run_multi_block_benchmark(
            &mut device,
            "/dev/test",
            TEST_SIZE,
            &[TEST_BLOCK, 2 * TEST_BLOCK],
            DataPattern::Zeros,
            true,
            true,
            Arc::new(AtomicBool::new(false)),
        )
        .expect("an uncancelled benchmark must succeed");
    }
}
