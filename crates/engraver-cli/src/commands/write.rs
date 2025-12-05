//! Write command - writes an image to a drive
//!
//! This is the main functionality of Engraver. It handles:
//! - Source validation (local files, URLs, compressed files)
//! - Target device validation and safety checks
//! - User confirmation
//! - Unmounting partitions
//! - Writing with progress display
//! - Optional verification

use anyhow::{bail, Context, Result};
use console::style;
use dialoguer::Confirm;
use indicatif::{ProgressBar, ProgressStyle};
use std::io::{Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use engraver_core::{
    validate_checkpoint, validate_source, CheckpointManager, ChecksumAlgorithm, Source, SourceType,
    Verifier, VerifyConfig, WriteCheckpoint, WriteConfig, Writer,
};
use engraver_detect::{list_drives, Drive};
use engraver_platform::{has_elevated_privileges, open_device, unmount_device, OpenOptions};

/// Arguments for the write command
pub struct WriteArgs {
    pub source: String,
    pub target: String,
    pub verify: bool,
    pub skip_confirm: bool,
    pub block_size: String,
    pub checksum: Option<String>,
    pub checksum_algo: String,
    pub force: bool,
    pub no_unmount: bool,
    pub cancel_flag: Arc<AtomicBool>,
    pub silent: bool,
    pub resume: bool,
    pub checkpoint: bool,
}

/// Conditionally print based on silent mode
macro_rules! print_if {
    ($silent:expr, $($arg:tt)*) => {
        if !$silent {
            print!($($arg)*);
        }
    };
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

/// Execute the write command
pub fn execute(args: WriteArgs) -> Result<()> {
    // Parse block size
    let block_size = parse_block_size(&args.block_size)?;
    let silent = args.silent;

    // Step 0: Check for elevated privileges
    if !has_elevated_privileges() {
        #[cfg(unix)]
        bail!(
            "Root privileges required.\n\
             Try running with: sudo engraver write ..."
        );

        #[cfg(windows)]
        bail!(
            "Administrator privileges required.\n\
             Right-click and select 'Run as administrator'."
        );

        #[cfg(not(any(unix, windows)))]
        bail!("Elevated privileges required for raw device access.");
    }

    // Step 1: Validate source
    println_if!(
        silent,
        "{} {}",
        style("Source:").bold(),
        style(&args.source).cyan()
    );

    let source_info = validate_source(&args.source)
        .with_context(|| format!("Failed to validate source: {}", args.source))?;

    let source_size = source_info.size.or(source_info.compressed_size);
    let source_type_str = match source_info.source_type {
        SourceType::LocalFile => "local file",
        SourceType::Remote => "remote URL",
        SourceType::Gzip => "gzip compressed",
        SourceType::Xz => "xz compressed",
        SourceType::Zstd => "zstd compressed",
        SourceType::Bzip2 => "bzip2 compressed",
    };

    if let Some(size) = source_size {
        println_if!(
            silent,
            "  {} ({}, {})",
            style("✓").green(),
            format_size(size),
            source_type_str
        );
    } else {
        println_if!(
            silent,
            "  {} (size unknown, {})",
            style("✓").green(),
            source_type_str
        );
    }

    // Step 2: Validate target device
    println_if!(
        silent,
        "\n{} {}",
        style("Target:").bold(),
        style(&args.target).cyan()
    );

    let drives = list_drives().context("Failed to list drives")?;
    let target_drive = find_drive(&drives, &args.target)?;

    // Safety check
    if target_drive.is_system && !args.force {
        bail!(
            "Refusing to write to system drive: {}\n\
             Reason: {}\n\n\
             If you really want to do this, use --force (DANGEROUS!)",
            target_drive.path,
            target_drive
                .system_reason
                .as_deref()
                .unwrap_or("Marked as system drive")
        );
    }

    // Warn if not safe target
    if !target_drive.is_safe_target() && !args.force {
        eprintln!(
            "{} Target drive is not marked as safe!",
            style("Warning:").yellow().bold()
        );
        if !args.skip_confirm {
            let proceed = Confirm::new()
                .with_prompt("Are you absolutely sure you want to continue?")
                .default(false)
                .interact()?;

            if !proceed {
                bail!("Aborted by user");
            }
        }
    }

    // Check size compatibility
    if let Some(src_size) = source_size {
        if src_size > target_drive.size {
            bail!(
                "Source ({}) is larger than target ({})",
                format_size(src_size),
                format_size(target_drive.size)
            );
        }
    }

    println_if!(
        silent,
        "  {} {} ({})",
        style("✓").green(),
        target_drive.display_name(),
        format_size(target_drive.size)
    );

    // Warn about slow USB connection speed
    if let Some(ref speed) = target_drive.usb_speed {
        if speed.is_slow() {
            println_if!(
                silent,
                "  {} USB speed: {} - connected at slower speed than capable",
                style("⚠").yellow().bold(),
                style(speed.to_string()).yellow()
            );
            println_if!(
                silent,
                "    {}",
                style("Tip: Try a USB 3.0 port for faster writes (look for blue USB ports)").dim()
            );
        } else {
            println_if!(
                silent,
                "  {} USB speed: {}",
                style("ℹ").blue(),
                style(speed.to_string()).green()
            );
        }
    }

    // Show mount points that will be unmounted
    if !target_drive.mount_points.is_empty() && !args.no_unmount {
        println_if!(
            silent,
            "  {} Will unmount: {}",
            style("⚠").yellow(),
            target_drive.mount_points.join(", ")
        );
    }

    // Step 3: Confirmation (skip_confirm is already true when silent)
    if !args.skip_confirm {
        println!();
        println!(
            "{}",
            style("╔════════════════════════════════════════════════════════════╗")
                .red()
                .bold()
        );
        println!(
            "{}",
            style("║                        WARNING                             ║")
                .red()
                .bold()
        );
        println!(
            "{}",
            style("║  ALL DATA ON THE TARGET DEVICE WILL BE PERMANENTLY LOST!   ║")
                .red()
                .bold()
        );
        println!(
            "{}",
            style("╚════════════════════════════════════════════════════════════╝")
                .red()
                .bold()
        );
        println!();

        let confirm_text = format!(
            "Write {} to {}?",
            source_info
                .path
                .split('/')
                .next_back()
                .unwrap_or(&source_info.path),
            target_drive.path
        );

        let proceed = Confirm::new()
            .with_prompt(confirm_text)
            .default(false)
            .interact()?;

        if !proceed {
            println!("{}", style("Aborted.").yellow());
            return Ok(());
        }
    }

    // Step 4: Unmount device using platform layer
    if !args.no_unmount {
        println_if!(silent, "\n{}", style("Unmounting device...").bold());

        // Use the device path for unmounting (unmount all partitions)
        match unmount_device(&target_drive.path) {
            Ok(()) => println_if!(silent, "  {} Device unmounted", style("✓").green()),
            Err(e) => {
                // Some platforms may return error if nothing to unmount
                tracing::debug!("Unmount result: {}", e);
                println_if!(silent, "  {} Unmount: {}", style("ℹ").blue(), e);
            }
        }
    }

    // Step 5: Verify source checksum (if provided)
    if let Some(expected_checksum) = &args.checksum {
        println_if!(silent, "\n{}", style("Verifying source checksum...").bold());

        let algo: ChecksumAlgorithm = args
            .checksum_algo
            .parse()
            .with_context(|| format!("Invalid checksum algorithm: {}", args.checksum_algo))?;

        let mut source_for_checksum =
            Source::open(&args.source).context("Failed to open source for checksum")?;

        let pb = create_progress_bar(source_size, "Checksumming", silent);

        let config = VerifyConfig::new().block_size(block_size);
        let pb_clone = pb.clone();
        let mut verifier = Verifier::with_config(config).on_progress(move |p| {
            pb_clone.set_position(p.bytes_processed);
        });

        let result = verifier.verify_checksum(
            &mut source_for_checksum,
            algo,
            expected_checksum,
            source_size,
        );

        pb.finish_and_clear();

        match result {
            Ok(_) => println_if!(
                silent,
                "  {} Checksum verified ({})",
                style("✓").green(),
                algo.name()
            ),
            Err(e) => bail!("Checksum verification failed: {}", e),
        }
    }

    // Step 6: Check for existing checkpoint (resume support)
    let checkpoint_manager = if args.checkpoint || args.resume {
        match CheckpointManager::default_location() {
            Ok(mgr) => Some(mgr),
            Err(e) => {
                tracing::warn!("Failed to create checkpoint manager: {}", e);
                None
            }
        }
    } else {
        None
    };

    let mut resume_offset: u64 = 0;
    let mut existing_checkpoint: Option<WriteCheckpoint> = None;

    if args.resume {
        if let Some(ref mgr) = checkpoint_manager {
            if let Ok(Some(checkpoint)) = mgr.find_checkpoint(&args.source, &target_drive.path) {
                // Validate the checkpoint
                let validation = validate_checkpoint(&checkpoint, &source_info, target_drive.size);

                if validation.valid {
                    // Show resume info
                    println_if!(
                        silent,
                        "\n{}",
                        style("Found checkpoint for resume:").bold().cyan()
                    );
                    println_if!(
                        silent,
                        "  Progress: {} / {} ({:.1}%)",
                        format_size(checkpoint.bytes_written),
                        checkpoint
                            .source_size
                            .map(format_size)
                            .unwrap_or_else(|| "unknown".to_string()),
                        checkpoint.percentage()
                    );
                    println_if!(silent, "  Resume count: {}", checkpoint.resume_count);

                    // Show any warnings
                    for warning in &validation.warnings {
                        println_if!(silent, "  {} {}", style("Warning:").yellow(), warning);
                    }

                    // Ask for confirmation unless skip_confirm is set
                    let should_resume = if args.skip_confirm {
                        true
                    } else {
                        Confirm::new()
                            .with_prompt("Resume from checkpoint?")
                            .default(true)
                            .interact()?
                    };

                    if should_resume {
                        resume_offset = checkpoint.bytes_written;
                        existing_checkpoint = Some(checkpoint);
                        println_if!(
                            silent,
                            "  {} Resuming from byte {}",
                            style("✓").green(),
                            resume_offset
                        );
                    } else {
                        // User chose not to resume, remove old checkpoint
                        let _ = mgr.remove(&checkpoint);
                        println_if!(silent, "  {} Starting fresh write", style("ℹ").blue());
                    }
                } else {
                    // Checkpoint is invalid, show why and remove it
                    println_if!(
                        silent,
                        "\n{}",
                        style("Existing checkpoint is invalid:").yellow()
                    );
                    for msg in &validation.messages {
                        println_if!(silent, "  {}", msg);
                    }
                    let _ = mgr.remove(&checkpoint);
                    println_if!(silent, "  {} Starting fresh write", style("ℹ").blue());
                }
            }
        }
    }

    // Step 7: Open source and target device
    println_if!(silent, "\n{}", style("Writing image...").bold());

    let mut source =
        Source::open_with_offset(&args.source, resume_offset).context("Failed to open source")?;

    // Open target device using platform layer with direct I/O
    let device_path = get_raw_device_path(&target_drive.path);
    let options = OpenOptions::new()
        .read(true)
        .write(true)
        .direct_io(true) // Bypass page cache for better performance
        .block_size(block_size);

    let mut target = open_device(&device_path, options)
        .with_context(|| format!("Failed to open device: {}", device_path))?;

    let device_info = target.info().clone();
    tracing::debug!(
        "Opened device: {} ({} bytes, block_size={}, direct_io={})",
        device_info.path,
        device_info.size,
        device_info.block_size,
        device_info.direct_io
    );

    // Step 8: Create or update checkpoint
    let mut checkpoint = if let Some(mut cp) = existing_checkpoint.take() {
        cp.mark_resumed();
        cp
    } else {
        let write_config = WriteConfig::new()
            .block_size(block_size)
            .sync_each_block(false)
            .sync_on_complete(true);
        WriteCheckpoint::new(
            &source_info,
            &target_drive.path,
            target_drive.size,
            &write_config,
        )
    };

    // Step 9: Write with progress and checkpointing
    let total_size = source_size.unwrap_or(0);
    let pb = create_write_progress_bar(total_size, silent);
    if resume_offset > 0 {
        pb.set_position(resume_offset);
    }

    let cancel_flag = args.cancel_flag.clone();

    let config = WriteConfig::new()
        .block_size(block_size)
        .sync_each_block(false)
        .sync_on_complete(true);

    let writer = Writer::with_config(config);

    // Set up progress callback with checkpoint saving
    let pb_clone = pb.clone();
    let last_checkpoint_bytes =
        std::sync::Arc::new(std::sync::atomic::AtomicU64::new(resume_offset));
    let last_checkpoint_clone = last_checkpoint_bytes.clone();

    let writer = writer.on_progress(move |progress| {
        pb_clone.set_position(progress.bytes_written);
        pb_clone.set_message(format!(
            "{}/s, ETA: {}",
            format_size(progress.speed_bps),
            progress.eta_display()
        ));

        // Track progress for checkpointing (checkpoint saved in main thread)
        last_checkpoint_clone.store(progress.bytes_written, Ordering::Relaxed);
    });

    // Connect cancel flag
    let writer_cancel = writer.cancel_handle();
    let cancel_flag_for_thread = cancel_flag.clone();
    std::thread::spawn(move || {
        while cancel_flag_for_thread.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        writer_cancel.store(true, Ordering::SeqCst);
    });

    let mut writer = writer;
    let start_time = Instant::now();

    // Use write_from_offset for resume support
    let write_result =
        writer.write_from_offset(&mut source, &mut *target, total_size, resume_offset);

    pb.finish_and_clear();

    // Handle write result
    let write_success = match &write_result {
        Ok(result) => {
            let elapsed = start_time.elapsed();
            let total_written = result.bytes_written;
            let resumed_bytes = if resume_offset > 0 { resume_offset } else { 0 };
            let session_bytes = total_written.saturating_sub(resumed_bytes);
            let speed = if elapsed.as_secs_f64() > 0.0 {
                session_bytes as f64 / elapsed.as_secs_f64()
            } else {
                0.0
            };

            if resume_offset > 0 {
                println_if!(
                    silent,
                    "  {} Wrote {} (resumed from {}) in {:.1}s ({}/s)",
                    style("✓").green(),
                    format_size(total_written),
                    format_size(resumed_bytes),
                    elapsed.as_secs_f64(),
                    format_size(speed as u64)
                );
            } else {
                println_if!(
                    silent,
                    "  {} Wrote {} in {:.1}s ({}/s)",
                    style("✓").green(),
                    format_size(total_written),
                    elapsed.as_secs_f64(),
                    format_size(speed as u64)
                );
            }
            true
        }
        Err(engraver_core::Error::Cancelled) => {
            // Save checkpoint on cancel
            if let Some(ref mgr) = checkpoint_manager {
                let bytes_written = last_checkpoint_bytes.load(Ordering::Relaxed);
                let blocks_written = bytes_written / block_size as u64;
                checkpoint.update_progress(bytes_written, blocks_written, start_time.elapsed());
                if let Err(e) = mgr.save(&checkpoint) {
                    tracing::warn!("Failed to save checkpoint: {}", e);
                } else {
                    println_if!(
                        silent,
                        "\n{} Checkpoint saved at {} bytes",
                        style("ℹ").blue(),
                        bytes_written
                    );
                    println_if!(silent, "  Run with --resume to continue");
                }
            }
            println_if!(silent, "\n{}", style("Write cancelled by user.").yellow());
            return Ok(());
        }
        Err(e) => {
            // Save checkpoint on error
            if let Some(ref mgr) = checkpoint_manager {
                let bytes_written = last_checkpoint_bytes.load(Ordering::Relaxed);
                let blocks_written = bytes_written / block_size as u64;
                checkpoint.update_progress(bytes_written, blocks_written, start_time.elapsed());
                if let Err(save_err) = mgr.save(&checkpoint) {
                    tracing::warn!("Failed to save checkpoint: {}", save_err);
                } else {
                    eprintln!(
                        "{} Checkpoint saved at {} bytes",
                        style("ℹ").blue(),
                        bytes_written
                    );
                    eprintln!("  Run with --resume to continue");
                }
            }
            bail!("Write failed: {}", e);
        }
    };

    // Remove checkpoint on success
    if write_success {
        if let Some(ref mgr) = checkpoint_manager {
            if let Err(e) = mgr.remove(&checkpoint) {
                tracing::warn!("Failed to remove checkpoint: {}", e);
            }
        }
    }

    // Step 10: Sync using platform layer
    print_if!(silent, "  Syncing... ");
    if !silent {
        std::io::stdout().flush()?;
    }
    target.sync().context("Failed to sync device")?;
    println_if!(silent, "{}", style("done").green());

    // Step 11: Verify (if requested)
    if args.verify {
        println_if!(silent, "\n{}", style("Verifying write...").bold());

        // For verification, we need a seekable source
        // For local uncompressed files, open directly
        // For remote/compressed, we recalculate checksum instead
        let source_is_local = source_info.source_type == SourceType::LocalFile;

        if source_is_local {
            // Direct byte-by-byte comparison for local files
            let mut source_file = std::fs::File::open(&args.source)
                .context("Failed to reopen source for verification")?;

            // Seek target back to start
            target.seek(SeekFrom::Start(0))?;

            let pb = create_progress_bar(source_size, "Verifying", silent);

            let config = VerifyConfig::new().block_size(block_size);
            let pb_clone = pb.clone();
            let mut verifier = Verifier::with_config(config).on_progress(move |p| {
                pb_clone.set_position(p.bytes_processed);
            });

            let verify_result = verifier.compare(&mut source_file, &mut *target, total_size);

            pb.finish_and_clear();

            match verify_result {
                Ok(result) if result.success => {
                    println_if!(
                        silent,
                        "  {} Verification passed ({} verified)",
                        style("✓").green(),
                        format_size(result.bytes_verified)
                    );
                }
                Ok(result) => {
                    bail!(
                        "Verification failed! {} mismatch(es) found. First mismatch at offset {}",
                        result.mismatches,
                        result.first_mismatch_offset.unwrap_or(0)
                    );
                }
                Err(e) => {
                    bail!("Verification failed: {}", e);
                }
            }
        } else {
            // For remote/compressed sources, verify via checksum
            println_if!(
                silent,
                "  {} Source is remote/compressed, using checksum verification",
                style("ℹ").blue()
            );

            // Calculate checksum of what we wrote
            target.seek(SeekFrom::Start(0))?;

            let pb = create_progress_bar(Some(total_size), "Checksumming", silent);

            let config = VerifyConfig::new().block_size(block_size);
            let pb_clone = pb.clone();
            let mut verifier = Verifier::with_config(config).on_progress(move |p| {
                pb_clone.set_position(p.bytes_processed);
            });

            let written_checksum = verifier
                .calculate_checksum(&mut *target, ChecksumAlgorithm::Sha256, Some(total_size))
                .context("Failed to checksum written data")?;

            pb.finish_and_clear();

            // Re-open source and calculate its checksum
            println_if!(silent, "  Calculating source checksum...");
            let mut source_for_checksum =
                Source::open(&args.source).context("Failed to reopen source")?;

            let pb = create_progress_bar(source_size, "Checksumming source", silent);

            let config = VerifyConfig::new().block_size(block_size);
            let pb_clone = pb.clone();
            let mut verifier = Verifier::with_config(config).on_progress(move |p| {
                pb_clone.set_position(p.bytes_processed);
            });

            let source_checksum = verifier
                .calculate_checksum(
                    &mut source_for_checksum,
                    ChecksumAlgorithm::Sha256,
                    source_size,
                )
                .context("Failed to checksum source")?;

            pb.finish_and_clear();

            if written_checksum.matches(&source_checksum) {
                println_if!(
                    silent,
                    "  {} Checksum verification passed",
                    style("✓").green()
                );
                println_if!(silent, "    SHA-256: {}", written_checksum.to_hex());
            } else {
                bail!(
                    "Checksum mismatch!\n  Source:  {}\n  Written: {}",
                    source_checksum.to_hex(),
                    written_checksum.to_hex()
                );
            }
        }
    }

    // Done!
    println_if!(silent);
    println_if!(
        silent,
        "{}",
        style("✓ Write complete! You can safely remove the drive.")
            .green()
            .bold()
    );

    Ok(())
}

/// Find a drive by path
fn find_drive<'a>(drives: &'a [Drive], path: &str) -> Result<&'a Drive> {
    // Normalize path for comparison
    let normalized = get_raw_device_path(path);

    for drive in drives {
        if drive.path == path || drive.path == normalized || drive.raw_path == path {
            return Ok(drive);
        }

        // Also check partition paths
        for part in &drive.partitions {
            if part.path == path {
                bail!(
                    "'{}' is a partition. Please specify the whole device: {}",
                    path,
                    drive.path
                );
            }
        }
    }

    bail!(
        "Device '{}' not found.\n\
         Run 'engraver list' to see available drives.",
        path
    )
}

/// Get the raw device path for a given device path
/// On macOS, converts /dev/disk2 to /dev/rdisk2 for raw access
fn get_raw_device_path(path: &str) -> String {
    #[cfg(target_os = "macos")]
    {
        if path.starts_with("/dev/disk") && !path.starts_with("/dev/rdisk") {
            return path.replace("/dev/disk", "/dev/rdisk");
        }
    }

    path.to_string()
}

/// Parse a human-readable block size (e.g., "4M", "1M", "512K")
fn parse_block_size(s: &str) -> Result<usize> {
    let s = s.trim().to_uppercase();

    let (num_str, multiplier) = if s.ends_with('K') {
        (&s[..s.len() - 1], 1024)
    } else if s.ends_with('M') {
        (&s[..s.len() - 1], 1024 * 1024)
    } else if s.ends_with('G') {
        (&s[..s.len() - 1], 1024 * 1024 * 1024)
    } else {
        (s.as_str(), 1)
    };

    let num: usize = num_str
        .parse()
        .with_context(|| format!("Invalid block size: {}", s))?;

    let size = num * multiplier;

    // Validate range
    if size < 4096 {
        bail!("Block size must be at least 4K");
    }
    if size > 64 * 1024 * 1024 {
        bail!("Block size must be at most 64M");
    }

    Ok(size)
}

/// Format a size in bytes to human-readable format
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.2} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Create a progress bar for operations
fn create_progress_bar(total: Option<u64>, operation: &str, silent: bool) -> ProgressBar {
    if silent {
        return ProgressBar::hidden();
    }

    let pb = match total {
        Some(t) => ProgressBar::new(t),
        None => ProgressBar::new_spinner(),
    };

    pb.set_style(
        ProgressStyle::default_bar()
            .template(&format!(
                "  {{spinner:.green}} {} [{{bar:40.cyan/blue}}] {{bytes}}/{{total_bytes}} ({{eta}})",
                operation
            ))
            .unwrap()
            .progress_chars("█▓░"),
    );

    pb
}

/// Create a progress bar for write operations
fn create_write_progress_bar(total: u64, silent: bool) -> ProgressBar {
    if silent {
        return ProgressBar::hidden();
    }

    let pb = if total > 0 {
        ProgressBar::new(total)
    } else {
        ProgressBar::new_spinner()
    };

    pb.set_style(
        ProgressStyle::default_bar()
            .template("  {spinner:.green} Writing [{bar:40.cyan/blue}] {bytes}/{total_bytes} {msg}")
            .unwrap()
            .progress_chars("█▓░"),
    );

    pb
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // parse_block_size tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_parse_block_size() {
        assert_eq!(parse_block_size("4096").unwrap(), 4096);
        assert_eq!(parse_block_size("4K").unwrap(), 4096);
        assert_eq!(parse_block_size("4k").unwrap(), 4096);
        assert_eq!(parse_block_size("1M").unwrap(), 1024 * 1024);
        assert_eq!(parse_block_size("4M").unwrap(), 4 * 1024 * 1024);
        assert_eq!(parse_block_size("64M").unwrap(), 64 * 1024 * 1024);
    }

    #[test]
    fn test_parse_block_size_with_whitespace() {
        assert_eq!(parse_block_size("  4K  ").unwrap(), 4096);
        assert_eq!(parse_block_size("\t1M\n").unwrap(), 1024 * 1024);
    }

    #[test]
    fn test_parse_block_size_gigabytes() {
        // 1G is valid (within 64M limit? No, 1G > 64M)
        assert!(parse_block_size("1G").is_err()); // Too large
    }

    #[test]
    fn test_parse_block_size_boundary_values() {
        // Minimum valid: 4K
        assert_eq!(parse_block_size("4K").unwrap(), 4096);
        assert!(parse_block_size("2K").is_err()); // Below minimum

        // Maximum valid: 64M
        assert_eq!(parse_block_size("64M").unwrap(), 64 * 1024 * 1024);
        assert!(parse_block_size("65M").is_err()); // Above maximum
    }

    #[test]
    fn test_parse_block_size_invalid() {
        assert!(parse_block_size("100").is_err()); // Too small
        assert!(parse_block_size("128M").is_err()); // Too large
        assert!(parse_block_size("abc").is_err()); // Invalid
        assert!(parse_block_size("").is_err()); // Empty
        assert!(parse_block_size("K").is_err()); // No number
        assert!(parse_block_size("-4K").is_err()); // Negative
    }

    // -------------------------------------------------------------------------
    // format_size tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1023), "1023 B");
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1024 * 1024), "1.00 MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(format_size(1024u64 * 1024 * 1024 * 1024), "1.00 TB");
    }

    #[test]
    fn test_format_size_fractional() {
        assert_eq!(format_size(1536), "1.50 KB"); // 1.5 KB
        assert_eq!(format_size(1024 * 1024 + 512 * 1024), "1.50 MB"); // 1.5 MB
    }

    #[test]
    fn test_format_size_large_values() {
        assert_eq!(format_size(10 * 1024 * 1024 * 1024), "10.00 GB");
        assert_eq!(format_size(500u64 * 1024 * 1024 * 1024), "500.00 GB");
        assert_eq!(format_size(2u64 * 1024 * 1024 * 1024 * 1024), "2.00 TB");
    }

    #[test]
    fn test_format_size_real_usb_sizes() {
        // Common USB drive sizes
        assert_eq!(format_size(4u64 * 1024 * 1024 * 1024), "4.00 GB");
        assert_eq!(format_size(8u64 * 1024 * 1024 * 1024), "8.00 GB");
        assert_eq!(format_size(16u64 * 1024 * 1024 * 1024), "16.00 GB");
        assert_eq!(format_size(32u64 * 1024 * 1024 * 1024), "32.00 GB");
        assert_eq!(format_size(64u64 * 1024 * 1024 * 1024), "64.00 GB");
        assert_eq!(format_size(128u64 * 1024 * 1024 * 1024), "128.00 GB");
        assert_eq!(format_size(256u64 * 1024 * 1024 * 1024), "256.00 GB");
    }

    // -------------------------------------------------------------------------
    // get_raw_device_path tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_get_raw_device_path() {
        #[cfg(target_os = "macos")]
        {
            assert_eq!(get_raw_device_path("/dev/disk2"), "/dev/rdisk2");
            assert_eq!(get_raw_device_path("/dev/rdisk2"), "/dev/rdisk2");
            assert_eq!(get_raw_device_path("/dev/disk0"), "/dev/rdisk0");
            assert_eq!(get_raw_device_path("/dev/disk10"), "/dev/rdisk10");
        }

        #[cfg(target_os = "linux")]
        {
            assert_eq!(get_raw_device_path("/dev/sdb"), "/dev/sdb");
            assert_eq!(get_raw_device_path("/dev/nvme0n1"), "/dev/nvme0n1");
            assert_eq!(get_raw_device_path("/dev/mmcblk0"), "/dev/mmcblk0");
        }
    }

    #[test]
    fn test_get_raw_device_path_passthrough() {
        // Non-device paths should pass through unchanged
        assert_eq!(get_raw_device_path("/tmp/test.img"), "/tmp/test.img");
        assert_eq!(get_raw_device_path("relative/path"), "relative/path");
    }

    // -------------------------------------------------------------------------
    // find_drive tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_find_drive_by_path() {
        let drives = vec![Drive {
            path: "/dev/sdb".to_string(),
            raw_path: "/dev/sdb".to_string(),
            name: "sdb".to_string(),
            size: 16 * 1024 * 1024 * 1024,
            removable: true,
            drive_type: engraver_detect::DriveType::Usb,
            vendor: Some("SanDisk".to_string()),
            model: Some("Ultra".to_string()),
            serial: None,
            partitions: vec![],
            mount_points: vec![],
            is_system: false,
            system_reason: None,
            usb_speed: None,
        }];

        let result = find_drive(&drives, "/dev/sdb");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().path, "/dev/sdb");
    }

    #[test]
    fn test_find_drive_not_found() {
        let drives = vec![Drive {
            path: "/dev/sda".to_string(),
            raw_path: "/dev/sda".to_string(),
            name: "sda".to_string(),
            size: 500 * 1024 * 1024 * 1024,
            removable: false,
            drive_type: engraver_detect::DriveType::Sata,
            vendor: None,
            model: None,
            serial: None,
            partitions: vec![],
            mount_points: vec![],
            is_system: true,
            system_reason: Some("Contains /".to_string()),
            usb_speed: None,
        }];

        let result = find_drive(&drives, "/dev/sdb");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"));
    }

    #[test]
    fn test_find_drive_partition_rejected() {
        let drives = vec![Drive {
            path: "/dev/sdb".to_string(),
            raw_path: "/dev/sdb".to_string(),
            name: "sdb".to_string(),
            size: 16 * 1024 * 1024 * 1024,
            removable: true,
            drive_type: engraver_detect::DriveType::Usb,
            vendor: None,
            model: None,
            serial: None,
            partitions: vec![engraver_detect::Partition {
                path: "/dev/sdb1".to_string(),
                size: 8 * 1024 * 1024 * 1024,
                filesystem: Some("vfat".to_string()),
                label: Some("UBUNTU".to_string()),
                mount_point: Some("/mnt/usb".to_string()),
            }],
            mount_points: vec!["/mnt/usb".to_string()],
            is_system: false,
            system_reason: None,
            usb_speed: None,
        }];

        // Trying to write to a partition should fail with helpful message
        let result = find_drive(&drives, "/dev/sdb1");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("partition"));
        assert!(err.contains("/dev/sdb"));
    }

    // -------------------------------------------------------------------------
    // Progress bar creation tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_create_progress_bar_silent() {
        let pb = create_progress_bar(Some(1024), "Test", true);
        // Silent progress bar should be hidden
        assert!(pb.is_hidden());
    }

    #[test]
    fn test_create_progress_bar_with_size() {
        let pb = create_progress_bar(Some(1024 * 1024), "Writing", false);
        // Progress bar may be hidden in test environment without a terminal
        // Just verify it was created with the right length
        assert_eq!(pb.length(), Some(1024 * 1024));
    }

    #[test]
    fn test_create_progress_bar_unknown_size() {
        let pb = create_progress_bar(None, "Downloading", false);
        // Spinner doesn't have a length - just verify creation succeeds
        assert!(pb.length().is_none() || pb.length() == Some(0));
    }

    #[test]
    fn test_create_write_progress_bar_silent() {
        let pb = create_write_progress_bar(1024, true);
        assert!(pb.is_hidden());
    }

    #[test]
    fn test_create_write_progress_bar_with_size() {
        let pb = create_write_progress_bar(1024 * 1024, false);
        // Progress bar may be hidden in test environment without a terminal
        // Just verify it was created with the right length
        assert_eq!(pb.length(), Some(1024 * 1024));
    }

    #[test]
    fn test_create_write_progress_bar_zero_size() {
        let pb = create_write_progress_bar(0, false);
        // Zero size creates a spinner - just verify creation succeeds
        // Spinner may have length 0 or None depending on indicatif version
        let _ = pb.length(); // Just ensure it doesn't panic
    }

    // -------------------------------------------------------------------------
    // WriteArgs struct tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_write_args_creation() {
        let args = WriteArgs {
            source: "ubuntu.iso".to_string(),
            target: "/dev/sdb".to_string(),
            verify: true,
            skip_confirm: false,
            block_size: "4M".to_string(),
            checksum: Some("abc123".to_string()),
            checksum_algo: "sha256".to_string(),
            force: false,
            no_unmount: false,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            silent: false,
            resume: false,
            checkpoint: true,
        };

        assert_eq!(args.source, "ubuntu.iso");
        assert_eq!(args.target, "/dev/sdb");
        assert!(args.verify);
        assert!(!args.skip_confirm);
        assert_eq!(args.block_size, "4M");
        assert!(args.checksum.is_some());
        assert!(!args.force);
        assert!(!args.cancel_flag.load(Ordering::Relaxed));
    }
}
