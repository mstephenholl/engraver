//! Benchmark command - tests write speed of a drive
//!
//! This command allows users to benchmark the write speed of a storage device
//! before committing to a potentially long write operation. It helps identify
//! slow drives or USB connections.
//!
//! **Warning:** This is a destructive operation that will overwrite data on the target device.

use anyhow::{bail, Context, Result};
use console::style;
use dialoguer::Confirm;
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

/// Arguments for the benchmark command
pub struct BenchmarkArgs {
    /// Target device path
    pub target: String,
    /// Test size (e.g., "256M", "1G")
    pub test_size: Option<String>,
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
    /// Silent mode (minimal output)
    pub silent: bool,
    /// Test multiple block sizes
    pub test_block_sizes: Option<String>,
    /// Cancellation flag
    pub cancel_flag: Arc<AtomicBool>,
}

/// Conditionally println based on silent mode
macro_rules! println_if {
    ($silent:expr) => {
        if !$silent {
            println!();
        }
    };
    ($silent:expr, $($arg:tt)*) => {
        if !$silent {
            println!($($arg)*);
        }
    };
}

/// Execute the benchmark command
pub fn execute(args: BenchmarkArgs) -> Result<()> {
    let silent = args.silent;

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

    // Calculate test size
    let base_test_size = if let Some(ref size_str) = args.test_size {
        parse_size(size_str).map_err(|e| anyhow::anyhow!("{}", e))?
    } else {
        256 * 1024 * 1024 // 256 MB default
    };

    // Step 4: Display configuration
    if !args.json {
        display_benchmark_info(
            target_drive,
            base_test_size,
            block_size,
            &pattern,
            args.passes,
            is_multi_block,
            &block_sizes,
            silent,
        );
    }

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
    let device = open_device(
        &target_drive.path,
        OpenOptions::new().write(true).direct_io(true),
    )
    .context("Failed to open device for writing")?;
    println_if!(silent, "  {} Device opened", style("✓").green());

    // Step 8: Run benchmark
    if is_multi_block {
        run_multi_block_benchmark(
            device,
            &target_drive.path,
            base_test_size,
            &block_sizes,
            pattern,
            args.json,
            silent,
            args.cancel_flag,
        )
    } else {
        run_single_benchmark(
            device,
            &target_drive.path,
            base_test_size,
            block_size,
            pattern,
            args.passes,
            args.json,
            silent,
            args.cancel_flag,
        )
    }
}

/// Validate command arguments before any I/O
fn validate_args(args: &BenchmarkArgs) -> Result<()> {
    // Check mutual exclusivity
    if args.test_size.is_some() && args.test_block_sizes.is_some() {
        bail!(
            "Cannot use both --size and --test-block-sizes options.\n\
             Use --size for a single block size benchmark, or --test-block-sizes to test multiple block sizes."
        );
    }

    // Validate test size if provided
    if let Some(ref size_str) = args.test_size {
        let size = parse_size(size_str).map_err(|e| anyhow::anyhow!("{}", e))?;
        if !is_power_of_two(size) {
            bail!("Test size must be a power of 2: {}", size_str);
        }
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
#[allow(clippy::too_many_arguments)]
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
        let effective_size =
            BenchmarkConfig::effective_test_size_for_block_sizes(test_size, block_sizes);
        println_if!(
            silent,
            "  Test size: {} (per block size)",
            format_size(effective_size)
        );
    } else {
        println_if!(silent, "  Test size: {}", format_size(test_size));
        println_if!(silent, "  Block size: {}", format_size(block_size));
    }

    let pattern_str = match pattern {
        DataPattern::Zeros => "zeros",
        DataPattern::Random => "random",
        DataPattern::Sequential => "sequential",
    };
    println_if!(silent, "  Pattern: {}", pattern_str);

    if !is_multi_block {
        println_if!(silent, "  Passes: {}", passes);
    }
}

/// Confirm benchmark with user
fn confirm_benchmark(drive: &engraver_detect::Drive) -> Result<bool> {
    println!();
    println!(
        "{}",
        style("╔══════════════════════════════════════════════════════════════╗")
            .red()
            .bold()
    );
    println!(
        "{}",
        style("║                     DESTRUCTIVE OPERATION                     ║")
            .red()
            .bold()
    );
    println!(
        "{}",
        style("║  This benchmark will OVERWRITE data on the target device!    ║")
            .red()
            .bold()
    );
    println!(
        "{}",
        style("║  ALL DATA ON THE TARGET DEVICE WILL BE PERMANENTLY LOST!     ║")
            .red()
            .bold()
    );
    println!(
        "{}",
        style("╚══════════════════════════════════════════════════════════════╝")
            .red()
            .bold()
    );
    println!();

    let size_str = format_size(drive.size);

    let confirm_text = format!(
        "Benchmark {} ({})? This WILL DESTROY ALL DATA on this drive",
        drive.path, size_str
    );

    Confirm::new()
        .with_prompt(confirm_text)
        .default(false)
        .interact()
        .context("Failed to get user confirmation")
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
#[allow(clippy::too_many_arguments)]
fn run_single_benchmark<W>(
    device: W,
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
    W: std::io::Write + std::io::Seek,
{
    let config = BenchmarkConfig {
        test_size,
        block_size,
        pattern,
        passes,
    };

    let effective_size = config.effective_test_size();
    let total_bytes = effective_size * passes as u64;

    println_if!(silent, "\n{} Benchmarking...", style("▶").cyan());

    // Create progress bar
    let pb = if silent || json {
        ProgressBar::hidden()
    } else {
        let pb = ProgressBar::new(total_bytes);
        pb.set_style(get_progress_style(0));
        pb
    };

    let runner = BenchmarkRunner::new(config);

    // Set up cancellation
    let runner_cancel = runner.cancel_handle();
    let cancel_flag_clone = Arc::clone(&cancel_flag);
    std::thread::spawn(move || {
        while !cancel_flag_clone.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        runner_cancel.store(true, Ordering::Relaxed);
    });

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
        Err(BenchmarkError::Cancelled) => {
            println_if!(silent, "\n{} Benchmark cancelled", style("✗").red());
            Ok(())
        }
        Err(e) => Err(anyhow::anyhow!("Benchmark failed: {}", e)),
    }
}

/// Run multi-block-size benchmark
#[allow(clippy::too_many_arguments)]
fn run_multi_block_benchmark<W>(
    mut device: W,
    device_path: &str,
    base_test_size: u64,
    block_sizes: &[u64],
    pattern: DataPattern,
    json: bool,
    silent: bool,
    cancel_flag: Arc<AtomicBool>,
) -> Result<()>
where
    W: std::io::Write + std::io::Seek,
{
    let effective_size =
        BenchmarkConfig::effective_test_size_for_block_sizes(base_test_size, block_sizes);
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
            println_if!(silent, "\n{} Benchmark cancelled", style("✗").red());
            return Ok(());
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
            test_size: effective_size,
            block_size,
            pattern,
            passes: 1,
        };

        // Seek to beginning
        device.seek(std::io::SeekFrom::Start(0))?;

        let pb = if silent || json {
            ProgressBar::hidden()
        } else {
            let pb = ProgressBar::new(effective_size);
            pb.set_style(get_progress_style(0));
            pb
        };

        let runner = BenchmarkRunner::new(test_config);

        let pb_clone = pb.clone();
        let result = runner.run(
            &mut device,
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
                    average_speed_bps: speed,
                    speed_display: engraver_core::benchmark::format_speed(speed),
                });
            }
            Err(BenchmarkError::Cancelled) => {
                println_if!(silent, "\n{} Benchmark cancelled", style("✗").red());
                return Ok(());
            }
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
