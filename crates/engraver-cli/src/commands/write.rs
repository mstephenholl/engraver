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
use indicatif::{ProgressBar, ProgressStyle};
use std::io::{Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use engraver_core::{
    auto_detect_checksum, validate_checkpoint, validate_source, CheckpointManager,
    ChecksumAlgorithm, Source, SourceType, Verifier, VerifyConfig, WriteCheckpoint, WriteConfig,
    WritePhase, Writer,
};
#[cfg(feature = "partition-info")]
use engraver_core::{inspect_from_buffer, read_partition_header, PartitionTableType};
use engraver_detect::{list_drives, Drive};
use engraver_platform::{has_elevated_privileges, open_device, unmount_device, OpenOptions};

use super::{
    cancelled, checksum_image_and_target, confirm, create_progress_bar, describe_source_size,
};

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
    pub auto_checksum: bool,
    pub show_partitions: bool,
}

/// Shared context for the write command's helper functions
struct WriteContext {
    silent: bool,
    block_size: usize,
}

/// Check that the process has elevated privileges, bail if not
fn check_privileges() -> Result<()> {
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
    Ok(())
}

/// Populate `source_info.source_header_hash` for local sources.
///
/// Used by resume validation: a checkpoint stores this hash, and on
/// resume the value is recomputed from the file currently at the same
/// path. A mismatch means the user has replaced the image, and
/// `validate_checkpoint` will refuse to resume.
///
/// Failures are non-fatal — a missing hash just falls back to the
/// path/size/etag checks. We trace the error and continue.
fn populate_source_header_hash(source_info: &mut engraver_core::SourceInfo, silent: bool) {
    match engraver_core::compute_local_header_hash(&source_info.path) {
        Ok(Some(hash)) => {
            tracing::debug!("Source header hash: {}", hash);
            source_info.source_header_hash = Some(hash);
        }
        Ok(None) => {
            // Remote / cloud source — etag covers content identity.
        }
        Err(e) => {
            tracing::warn!(
                "Could not compute source header hash (resume safety reduced): {}",
                e
            );
            if !silent {
                eprintln!(
                    "  {} could not compute source content hash: {}",
                    style("warning:").yellow(),
                    e
                );
            }
        }
    }
}

/// Validate the source image and display info
fn validate_source_info(source: &str, silent: bool) -> Result<engraver_core::SourceInfo> {
    println_if!(
        silent,
        "{} {}",
        style("Source:").bold(),
        style(source).cyan()
    );

    let source_info = validate_source(source)
        .with_context(|| format!("Failed to validate source: {}", source))?;

    let source_type_str = match source_info.source_type {
        SourceType::LocalFile => "local file",
        SourceType::Remote => "remote URL",
        SourceType::Gzip => "gzip",
        SourceType::Xz => "xz",
        SourceType::Zstd => "zstd",
        SourceType::Bzip2 => "bzip2",
        #[cfg(feature = "s3")]
        SourceType::S3 => "S3 object",
        #[cfg(feature = "gcs")]
        SourceType::Gcs => "GCS object",
        #[cfg(feature = "azure")]
        SourceType::Azure => "Azure blob",
        _ => "unknown",
    };

    let size = describe_source_size(&source_info, format_size)
        .unwrap_or_else(|| "size unknown".to_string());
    println_if!(
        silent,
        "  {} ({}, {})",
        style("✓").green(),
        size,
        source_type_str
    );

    Ok(source_info)
}

/// Whether an image fits on a drive
#[derive(Debug, PartialEq, Eq)]
enum Capacity {
    /// The image is known to fit
    Fits,
    /// The image, of this many bytes, is larger than the drive
    TooLarge(u64),
    /// The compressed file alone, of this many bytes, is larger than the drive
    CompressedTooLarge(u64),
    /// The image length is not known, so it cannot be told
    Unknown,
}

/// Judge whether the source's image fits on a drive of `drive_size` bytes
///
/// A source with no image length is `Unknown`, unless its compressed file
/// alone is larger than the drive.
fn capacity(source_info: &engraver_core::SourceInfo, drive_size: u64) -> Capacity {
    match (source_info.size, source_info.compressed_size) {
        (Some(size), _) if size > drive_size => Capacity::TooLarge(size),
        (Some(_), _) => Capacity::Fits,
        (None, Some(compressed)) if compressed > drive_size => {
            Capacity::CompressedTooLarge(compressed)
        }
        (None, _) => Capacity::Unknown,
    }
}

/// Validate the target device, performing safety checks
fn validate_target_device<'a>(
    target: &str,
    drives: &'a [Drive],
    force: bool,
    skip_confirm: bool,
    source_info: &engraver_core::SourceInfo,
    silent: bool,
) -> Result<&'a Drive> {
    println_if!(
        silent,
        "\n{} {}",
        style("Target:").bold(),
        style(target).cyan()
    );

    let target_drive = find_drive(drives, target)?;

    // Safety check
    if target_drive.is_system && !force {
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
    if !target_drive.is_safe_target() && !force {
        eprintln!(
            "{} Target drive is not marked as safe!",
            style("Warning:").yellow().bold()
        );
        if !skip_confirm {
            let proceed = confirm("Are you absolutely sure you want to continue?", false)?;

            if !proceed {
                bail!("Aborted by user");
            }
        }
    }

    // Check size compatibility
    match capacity(source_info, target_drive.size) {
        Capacity::TooLarge(size) => bail!(
            "Source ({}) is larger than target ({})",
            format_size(size),
            format_size(target_drive.size)
        ),
        Capacity::CompressedTooLarge(compressed) => bail!(
            "Compressed source alone ({}) is larger than target ({})",
            format_size(compressed),
            format_size(target_drive.size)
        ),
        Capacity::Unknown => println_if!(
            silent,
            "  {} Image size is unknown until it is decompressed, so not checking that it fits",
            style("ℹ").blue()
        ),
        Capacity::Fits => {}
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
    if !target_drive.mount_points.is_empty() {
        println_if!(
            silent,
            "  {} Will unmount: {}",
            style("⚠").yellow(),
            target_drive.mount_points.join(", ")
        );
    }

    Ok(target_drive)
}

/// Display the confirmation dialog and return whether to proceed
fn confirm_write(
    source_info: &engraver_core::SourceInfo,
    drive: &Drive,
    skip_confirm: bool,
) -> Result<bool> {
    if skip_confirm {
        return Ok(true);
    }

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
        drive.path
    );

    let proceed = confirm(&confirm_text, false)?;

    if !proceed {
        println!("{}", style("Aborted.").yellow());
    }

    Ok(proceed)
}

/// Unmount the target device
fn unmount_target(path: &str, silent: bool) {
    println_if!(silent, "\n{}", style("Unmounting device...").bold());

    match unmount_device(path) {
        Ok(()) => println_if!(silent, "  {} Device unmounted", style("✓").green()),
        Err(e) => {
            tracing::debug!("Unmount result: {}", e);
            println_if!(silent, "  {} Unmount: {}", style("ℹ").blue(), e);
        }
    }
}

/// Auto-detect or use explicit checksum, verify if found
fn setup_checksum(args: &WriteArgs, image_size: Option<u64>, ctx: &WriteContext) -> Result<()> {
    let (effective_checksum, effective_algo) = if args.checksum.is_none() && args.auto_checksum {
        if let Some(detected) = auto_detect_checksum(&args.source) {
            println_if!(
                ctx.silent,
                "\n{} Found checksum file: {}",
                style("✓").green(),
                detected.source_file.display()
            );
            (Some(detected.checksum), Some(detected.algorithm))
        } else {
            println_if!(
                ctx.silent,
                "\n{} No checksum file found (tried .sha256, .sha512, .md5, SHA256SUMS, etc.)",
                style("ℹ").blue()
            );
            (None, None)
        }
    } else {
        let algo = args.checksum.as_ref().map(|_| {
            args.checksum_algo
                .parse::<ChecksumAlgorithm>()
                .unwrap_or(ChecksumAlgorithm::Sha256)
        });
        (args.checksum.clone(), algo)
    };

    if let Some(expected_checksum) = &effective_checksum {
        println_if!(
            ctx.silent,
            "\n{}",
            style("Verifying source checksum...").bold()
        );

        let algo: ChecksumAlgorithm = effective_algo.unwrap_or_else(|| {
            args.checksum_algo
                .parse()
                .unwrap_or(ChecksumAlgorithm::Sha256)
        });

        let mut source_for_checksum =
            Source::open(&args.source).context("Failed to open source for checksum")?;

        let pb = create_progress_bar(image_size, "Checksumming", ctx.silent);

        let config = VerifyConfig::new().block_size(ctx.block_size);
        let pb_clone = pb.clone();
        let mut verifier = Verifier::with_config(config)
            .with_cancel_flag(Arc::clone(&args.cancel_flag))
            .on_progress(move |p| {
                pb_clone.set_position(p.bytes_processed);
            });

        let result = verifier.verify_checksum(
            &mut source_for_checksum,
            algo,
            expected_checksum,
            image_size,
        );

        pb.finish_and_clear();

        match result {
            Ok(_) => println_if!(
                ctx.silent,
                "  {} Checksum verified ({})",
                style("✓").green(),
                algo.name()
            ),
            Err(engraver_core::Error::Cancelled) => {
                return Err(cancelled(
                    "Cancelled before writing. The target was not modified.",
                ));
            }
            Err(e) => bail!("Checksum verification failed: {}", e),
        }
    }

    Ok(())
}

/// Set up checkpoint manager and handle resume logic
fn setup_checkpoint(
    args: &WriteArgs,
    source_info: &engraver_core::SourceInfo,
    target_drive: &Drive,
) -> Result<(Option<CheckpointManager>, u64, Option<WriteCheckpoint>)> {
    let silent = args.silent;
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
                let validation = validate_checkpoint(&checkpoint, source_info, target_drive.size);

                if validation.valid {
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

                    for warning in &validation.warnings {
                        println_if!(silent, "  {} {}", style("Warning:").yellow(), warning);
                    }

                    let should_resume = if args.skip_confirm {
                        true
                    } else {
                        confirm("Resume from checkpoint?", true)?
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
                        let _ = mgr.remove(&checkpoint);
                        println_if!(silent, "  {} Starting fresh write", style("ℹ").blue());
                    }
                } else {
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

    Ok((checkpoint_manager, resume_offset, existing_checkpoint))
}

/// Execute the write command
pub fn execute(args: WriteArgs) -> Result<()> {
    let block_size = engraver_core::parse_block_size(&args.block_size)? as usize;
    let silent = args.silent;
    let ctx = WriteContext { silent, block_size };

    // Step 0: Check for elevated privileges
    check_privileges()?;

    // Step 1: Validate source
    let mut source_info = validate_source_info(&args.source, silent)?;

    // Compute a content fingerprint for resume validation. For local
    // files we hash the first 1 MB so a later resume can detect when the
    // file at `source_path` has been replaced. For remote / cloud
    // sources, the etag stored in `source_info` already serves this role.
    populate_source_header_hash(&mut source_info, silent);

    // Step 2: Validate target device
    let drives = list_drives().context("Failed to list drives")?;
    let target_drive = validate_target_device(
        &args.target,
        &drives,
        args.force,
        args.skip_confirm,
        &source_info,
        silent,
    )?;

    // Step 2.5: Show partition information if requested
    if args.show_partitions {
        display_source_partitions(&args.source, silent)?;
    }

    // Step 3: Confirmation
    if !confirm_write(&source_info, target_drive, args.skip_confirm)? {
        return Ok(());
    }

    // Step 4: Unmount device
    if !args.no_unmount {
        unmount_target(&target_drive.path, silent);
    }

    // Step 5: Checksum verification
    setup_checksum(&args, source_info.size, &ctx)?;

    // Step 6: Check for existing checkpoint (resume support)
    let (checkpoint_manager, resume_offset, mut existing_checkpoint) =
        setup_checkpoint(&args, &source_info, target_drive)?;

    // Step 7: Open source and target device
    let image_size = source_info.size;
    let total_blocks = image_size
        .map(|s| s.div_ceil(block_size as u64).to_string())
        .unwrap_or_else(|| "unknown".to_string());
    println_if!(silent, "\n{}", style("Writing image...").bold());
    println_if!(
        silent,
        "  {} Block size: {}, Total blocks: {}",
        style("ℹ").blue(),
        format_size(block_size as u64),
        total_blocks
    );

    let mut source =
        Source::open_with_offset(&args.source, resume_offset).context("Failed to open source")?;

    // Open target device using platform layer with direct I/O
    let device_path = get_raw_device_path(&target_drive.path);
    // The device alignment stays at its 4096 default. The Writer's chunk buffer
    // is 4096-aligned, not block_size-aligned, so a larger value would send
    // every chunk through the device's bounce buffer.
    let options = OpenOptions::new().read(true).write(true).direct_io(true); // Bypass page cache for better performance

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
    let write_size = image_size.unwrap_or(0);
    let pb = create_write_progress_bar(write_size, silent);
    if resume_offset > 0 {
        pb.set_position(resume_offset);
    }

    let cancel_flag = args.cancel_flag.clone();

    // Enable parallel verification: hash source data during write, then read back
    // target to verify. Cannot be used with resume (partial hash would be incorrect).
    let use_parallel_verify = args.verify && resume_offset == 0;
    let verify_algo: ChecksumAlgorithm = args
        .checksum_algo
        .parse()
        .unwrap_or(ChecksumAlgorithm::Sha256);

    let mut config = WriteConfig::new()
        .block_size(block_size)
        .sync_each_block(false)
        .sync_on_complete(true);

    if use_parallel_verify {
        config = config.checksum_algorithm(Some(verify_algo));
    }

    let writer = Writer::with_config(config);

    // Set up progress callback with checkpoint saving
    let pb_clone = pb.clone();
    let last_checkpoint_bytes =
        std::sync::Arc::new(std::sync::atomic::AtomicU64::new(resume_offset));
    let last_checkpoint_clone = last_checkpoint_bytes.clone();
    let phase_switched = std::sync::Arc::new(AtomicBool::new(false));
    let phase_switched_clone = phase_switched.clone();

    let writer = writer.on_progress(move |progress| {
        // When the phase switches to Verifying, update the progress bar style
        if progress.phase == WritePhase::Verifying
            && !phase_switched_clone.swap(true, Ordering::Relaxed)
        {
            pb_clone.set_position(0);
            pb_clone.set_style(
                ProgressStyle::default_bar()
                    .template("  {spinner:.green} Verifying [{bar:40.cyan/blue}] {bytes}/{total_bytes} {msg}")
                    .unwrap()
                    .progress_chars("█▓░"),
            );
        }

        pb_clone.set_position(progress.bytes_written);

        // Build detailed progress message
        let mut msg = format!(
            "{}/s | Block {}/{} | ETA: {}",
            format_size(progress.speed_bps),
            progress.current_block,
            progress.total_blocks,
            progress.eta_display()
        );

        // Show retry count if any retries have occurred (only during write phase)
        if progress.retry_count > 0 {
            msg.push_str(&format!(" | {} retries", progress.retry_count));
        }

        pb_clone.set_message(msg);

        // Track progress for checkpointing (checkpoint saved in main thread)
        if progress.phase == WritePhase::Writing {
            last_checkpoint_clone.store(progress.bytes_written, Ordering::Relaxed);
        }
    });

    // Share the program-wide cancel flag directly with the writer.
    // Convention is `true = cancel` on both sides, so a single
    // store(true) from the Ctrl+C handler reaches the writer's loop on
    // its next block boundary — no polling thread to bridge two
    // separately-named atomics.
    let mut writer = writer.with_cancel_flag(cancel_flag);
    let start_time = Instant::now();

    // Use write_and_verify for parallel verification, write_from_offset otherwise
    let write_result = if use_parallel_verify {
        writer.write_and_verify(&mut source, &mut *target, write_size)
    } else {
        writer.write_from_offset(&mut source, &mut *target, write_size, resume_offset)
    };

    pb.finish_and_clear();

    // Handle write result
    let image_written = match &write_result {
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

            // Calculate blocks written
            let blocks_written = total_written.div_ceil(block_size as u64);

            // Build retry info if any retries occurred
            let retry_info = if result.retry_count > 0 {
                format!(", {} retries", result.retry_count)
            } else {
                String::new()
            };

            if resume_offset > 0 {
                let resumed_blocks = resumed_bytes.div_ceil(block_size as u64);
                println_if!(
                    silent,
                    "  {} Wrote {} ({} blocks, resumed from {} / {} blocks) in {:.1}s ({}/s){}",
                    style("✓").green(),
                    format_size(total_written),
                    blocks_written,
                    format_size(resumed_bytes),
                    resumed_blocks,
                    elapsed.as_secs_f64(),
                    format_size(speed as u64),
                    retry_info
                );
            } else {
                println_if!(
                    silent,
                    "  {} Wrote {} ({} blocks) in {:.1}s ({}/s){}",
                    style("✓").green(),
                    format_size(total_written),
                    blocks_written,
                    elapsed.as_secs_f64(),
                    format_size(speed as u64),
                    retry_info
                );
            }

            // Report parallel verification results if used
            if let Some(verified) = result.verified {
                let verify_elapsed = result
                    .verification_elapsed
                    .map(|d| format!("{:.1}s", d.as_secs_f64()))
                    .unwrap_or_else(|| "?".to_string());
                if verified {
                    println_if!(
                        silent,
                        "  {} Verification passed ({}) in {}",
                        style("✓").green(),
                        verify_algo,
                        verify_elapsed
                    );
                    if let Some(ref checksum) = result.source_checksum {
                        println_if!(silent, "    {}", checksum);
                    }
                } else {
                    bail!(
                        "Verification failed!\n\
                         Source checksum:  {}\n\
                         Written checksum: {}\n\
                         \n\
                         The written data doesn't match the source.\n\
                         \n\
                         Suggestions:\n\
                         • Try writing again to a different device\n\
                         • Use a different USB port (preferably USB 3.0)",
                        result.source_checksum.as_deref().unwrap_or("unknown"),
                        result.target_checksum.as_deref().unwrap_or("unknown")
                    );
                }
            }

            result.bytes_written
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
                        "\n{} Checkpoint saved: {} ({} blocks written)",
                        style("ℹ").blue(),
                        format_size(bytes_written),
                        blocks_written
                    );
                    println_if!(silent, "  Run with --resume to continue from this point");
                }
            }
            // Sync to flush any pending writes before returning
            if let Err(e) = target.sync() {
                tracing::debug!("Sync after cancel: {}", e);
            }
            return Err(cancelled("Write cancelled by user."));
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
                        "{} Checkpoint saved: {} ({} blocks written)",
                        style("ℹ").blue(),
                        format_size(bytes_written),
                        blocks_written
                    );
                    eprintln!("  Run with --resume to continue from this point");
                }
            }
            // Sync to flush any pending writes before bailing
            if let Err(sync_err) = target.sync() {
                tracing::debug!("Sync after error: {}", sync_err);
            }
            // Provide user-friendly error with suggestions
            bail!("{}", format_write_error(e));
        }
    };

    // Remove checkpoint on success
    if let Some(ref mgr) = checkpoint_manager {
        if let Err(e) = mgr.remove(&checkpoint) {
            tracing::warn!("Failed to remove checkpoint: {}", e);
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
    // Skip if parallel verification already completed during write
    if args.verify && !use_parallel_verify {
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

            let pb = create_progress_bar(image_size, "Verifying", silent);

            let config = VerifyConfig::new().block_size(block_size);
            let pb_clone = pb.clone();
            let verify_block_size = block_size;
            let mut verifier = Verifier::with_config(config)
                .with_cancel_flag(args.cancel_flag.clone())
                .on_progress(move |p| {
                    pb_clone.set_position(p.bytes_processed);
                    let blocks = p.bytes_processed.div_ceil(verify_block_size as u64);
                    let total = p
                        .total_bytes
                        .map(|t| t.div_ceil(verify_block_size as u64))
                        .unwrap_or(0);
                    pb_clone.set_message(format!(
                        "{}/s | Block {}/{} | ETA: {}",
                        p.speed_display(),
                        blocks,
                        total,
                        p.eta_display()
                    ));
                });

            let verify_result = verifier.compare(&mut source_file, &mut *target, write_size);

            pb.finish_and_clear();

            match verify_result {
                Ok(result) if result.success => {
                    let blocks_verified = result.bytes_verified.div_ceil(block_size as u64);
                    println_if!(
                        silent,
                        "  {} Verification passed: {} ({} blocks) in {:.1}s ({}/s)",
                        style("✓").green(),
                        format_size(result.bytes_verified),
                        blocks_verified,
                        result.elapsed.as_secs_f64(),
                        format_size(result.speed_bps)
                    );
                }
                Ok(result) => {
                    bail!(
                        "Verification failed! {} mismatch(es) found.\n\
                         First mismatch at offset {} (block {})\n\
                         \n\
                         This indicates the written data doesn't match the source.\n\
                         \n\
                         Possible causes:\n\
                         • Faulty USB drive or SD card\n\
                         • Poor quality or counterfeit storage device\n\
                         • USB connection issues\n\
                         \n\
                         Suggestions:\n\
                         • Try writing again to a different device\n\
                         • Use a different USB port (preferably USB 3.0)",
                        result.mismatches,
                        result.first_mismatch_offset.unwrap_or(0),
                        result.first_mismatch_offset.unwrap_or(0) / block_size as u64
                    );
                }
                Err(engraver_core::Error::Cancelled) => {
                    return Err(cancelled("Verification cancelled."));
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

            target.seek(SeekFrom::Start(0))?;

            let (source_checksum, written_checksum) = checksum_image_and_target(
                &args.source,
                &mut *target,
                Some(image_written),
                block_size,
                &args.cancel_flag,
                silent,
            )?;

            if written_checksum.matches(&source_checksum) {
                println_if!(
                    silent,
                    "  {} Checksum verification passed (SHA-256)",
                    style("✓").green()
                );
                println_if!(silent, "    {}", written_checksum.to_hex());
            } else {
                bail!(
                    "Checksum mismatch!\n\
                     Source:  {}\n\
                     Written: {}\n\
                     \n\
                     The written data doesn't match the source.\n\
                     \n\
                     Possible causes:\n\
                     • Faulty USB drive or SD card\n\
                     • Data corruption during write\n\
                     • Source file changed during operation\n\
                     \n\
                     Suggestions:\n\
                     • Try writing again\n\
                     • Use a different device",
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

/// Format a write error with user-friendly suggestions
fn format_write_error(error: &engraver_core::Error) -> String {
    use engraver_core::Error;

    match error {
        Error::Io(io_err) => format_io_error(io_err),
        Error::PermissionDenied(msg) => {
            #[cfg(unix)]
            {
                format!(
                    "Permission denied: {}\n\
                     \n\
                     Suggestions:\n\
                     • Run with elevated privileges: sudo engraver write ...\n\
                     • Check that you have write access to the device\n\
                     • Ensure the device is not mounted read-only",
                    msg
                )
            }
            #[cfg(windows)]
            {
                format!(
                    "Permission denied: {}\n\
                     \n\
                     Suggestions:\n\
                     • Run as Administrator (right-click and select 'Run as administrator')\n\
                     • Check that no other program is using the device\n\
                     • Try closing any file explorer windows showing the drive",
                    msg
                )
            }
        }
        Error::DeviceBusy(msg) => {
            format!(
                "Device is busy: {}\n\
                 \n\
                 Suggestions:\n\
                 • Close any applications using the device\n\
                 • Unmount all partitions on the device\n\
                 • On Linux: lsof {} | grep -v COMMAND\n\
                 • On macOS: lsof | grep <device>\n\
                 • Try running with --force to skip safety checks (use with caution)",
                msg, msg
            )
        }
        Error::PartialWrite { expected, actual } => {
            format!(
                "Partial write: expected {} bytes, wrote {} bytes\n\
                 \n\
                 This usually indicates:\n\
                 • Device write errors (try a different USB port)\n\
                 • Device is failing or of poor quality\n\
                 • USB connection issues (try a shorter cable)\n\
                 \n\
                 Suggestions:\n\
                 • Run with --resume to retry from the last checkpoint\n\
                 • Try a smaller block size: --block-size 1M\n\
                 • Check dmesg/system logs for device errors",
                expected, actual
            )
        }
        Error::SourceTooShort { .. } | Error::SourceTooLong { .. } => {
            format!(
                "{}\n\
                 \n\
                 The source did not match its expected length.\n\
                 \n\
                 This usually indicates:\n\
                 • A truncated or incomplete download\n\
                 • The source file changed while it was being written\n\
                 • The server returned a different file than the one that was checked\n\
                 \n\
                 Suggestions:\n\
                 • Download the image again and compare its checksum\n\
                 • Make sure nothing is modifying the source file\n\
                 • Start a fresh write instead of resuming",
                error
            )
        }
        Error::DeviceNotFound(path) => {
            format!(
                "Device not found: {}\n\
                 \n\
                 Suggestions:\n\
                 • Check that the device is connected\n\
                 • Run 'engraver list' to see available devices\n\
                 • Verify the device path is correct\n\
                 • On Linux, device paths are typically /dev/sdX or /dev/nvmeXnY\n\
                 • On macOS, device paths are typically /dev/diskN",
                path
            )
        }
        Error::SizeMismatch {
            source_size,
            target_size,
        } => {
            format!(
                "Size mismatch: source is {} bytes, target device is {} bytes\n\
                 \n\
                 The target device is smaller than the source image.\n\
                 \n\
                 Suggestions:\n\
                 • Use a larger USB drive or SD card\n\
                 • Verify you selected the correct source image\n\
                 • Check if the image is compressed and needs more space",
                source_size, target_size
            )
        }
        Error::VerificationFailed {
            offset,
            expected,
            actual,
        } => {
            format!(
                "Verification failed at offset {}: expected {}, got {}\n\
                 \n\
                 The written data does not match the source.\n\
                 \n\
                 Possible causes:\n\
                 • Faulty USB drive or SD card\n\
                 • Poor quality or counterfeit storage device\n\
                 • USB connection issues\n\
                 \n\
                 Suggestions:\n\
                 • Try writing again to a different device\n\
                 • Use a different USB port (preferably USB 3.0)\n\
                 • Test the device with 'engraver benchmark <device>'",
                offset, expected, actual
            )
        }
        Error::Network { ref message, .. } => {
            format!(
                "Network error: {}\n\
                 \n\
                 Suggestions:\n\
                 • Check your internet connection\n\
                 • Verify the URL is accessible in a browser\n\
                 • If using a proxy, ensure it's configured correctly\n\
                 • Try downloading the file locally first, then write it",
                message
            )
        }
        Error::Decompression { ref message, .. } => {
            format!(
                "Decompression error: {}\n\
                 \n\
                 The compressed file may be corrupted.\n\
                 \n\
                 Suggestions:\n\
                 • Verify the file checksum matches the expected value\n\
                 • Re-download the file if it's from the internet\n\
                 • Try decompressing manually to verify the file\n\
                 • Ensure you have enough disk space for decompression",
                message
            )
        }
        Error::Cancelled => "Operation cancelled".to_string(),
        _ => format!("Write failed: {}", error),
    }
}

/// Format an IO error with user-friendly suggestions
fn format_io_error(io_err: &std::io::Error) -> String {
    use std::io::ErrorKind;

    let kind = io_err.kind();
    let base_msg = io_err.to_string();

    match kind {
        ErrorKind::PermissionDenied => {
            #[cfg(unix)]
            {
                format!(
                    "Permission denied: {}\n\
                     \n\
                     Suggestions:\n\
                     • Run with sudo: sudo engraver write ...\n\
                     • Check device permissions with: ls -la <device>\n\
                     • Ensure you're in the 'disk' group (Linux)",
                    base_msg
                )
            }
            #[cfg(windows)]
            {
                format!(
                    "Permission denied: {}\n\
                     \n\
                     Suggestions:\n\
                     • Run as Administrator\n\
                     • Close any programs using the device\n\
                     • Check if the device is write-protected",
                    base_msg
                )
            }
        }
        ErrorKind::NotFound => {
            format!(
                "Device not found: {}\n\
                 \n\
                 Suggestions:\n\
                 • Check that the device is connected\n\
                 • Run 'engraver list' to see available devices\n\
                 • The device may have been disconnected during the operation",
                base_msg
            )
        }
        ErrorKind::InvalidInput => {
            format!(
                "Invalid input: {}\n\
                 \n\
                 Suggestions:\n\
                 • Check the block size is valid (4K-64M)\n\
                 • Verify the source file path is correct",
                base_msg
            )
        }
        ErrorKind::WriteZero => {
            format!(
                "Write returned zero bytes: {}\n\
                 \n\
                 The device stopped accepting data.\n\
                 \n\
                 Possible causes:\n\
                 • Device is full\n\
                 • Device was disconnected\n\
                 • Device is failing\n\
                 \n\
                 Suggestions:\n\
                 • Check the device connection\n\
                 • Try a different USB port\n\
                 • Test the device with 'engraver benchmark <device>'",
                base_msg
            )
        }
        ErrorKind::Interrupted => {
            format!(
                "Operation interrupted: {}\n\
                 \n\
                 The write was interrupted by a signal.\n\
                 \n\
                 Suggestions:\n\
                 • Run with --resume to continue from the last checkpoint",
                base_msg
            )
        }
        ErrorKind::TimedOut => {
            format!(
                "Operation timed out: {}\n\
                 \n\
                 The device is not responding.\n\
                 \n\
                 Possible causes:\n\
                 • Device is very slow or unresponsive\n\
                 • USB connection issues\n\
                 • Device may be failing\n\
                 \n\
                 Suggestions:\n\
                 • Try a different USB port (USB 3.0 if available)\n\
                 • Use a shorter or better quality USB cable\n\
                 • Test the device with 'engraver benchmark <device>'",
                base_msg
            )
        }
        _ => {
            // Check for specific OS error codes
            #[cfg(unix)]
            if let Some(os_err) = io_err.raw_os_error() {
                return format_unix_error(os_err, &base_msg);
            }

            #[cfg(windows)]
            if let Some(os_err) = io_err.raw_os_error() {
                return format_windows_error(os_err, &base_msg);
            }

            format!(
                "IO error: {}\n\
                 \n\
                 Suggestions:\n\
                 • Check the device is still connected\n\
                 • Run with --resume to continue from the last checkpoint\n\
                 • Check system logs for more details",
                base_msg
            )
        }
    }
}

/// Format Unix-specific error codes
#[cfg(unix)]
fn format_unix_error(errno: i32, base_msg: &str) -> String {
    match errno {
        5 => {
            // EIO - Input/output error
            format!(
                "I/O error: {}\n\
                 \n\
                 A hardware I/O error occurred.\n\
                 \n\
                 Possible causes:\n\
                 • Bad sectors on the device\n\
                 • Device is failing\n\
                 • USB connection issues\n\
                 \n\
                 Suggestions:\n\
                 • Try a different USB port\n\
                 • Use a different device\n\
                 • Check dmesg for detailed error messages:\n\
                   dmesg | tail -50",
                base_msg
            )
        }
        16 => {
            // EBUSY - Device or resource busy
            format!(
                "Device busy: {}\n\
                 \n\
                 Another process is using the device.\n\
                 \n\
                 Suggestions:\n\
                 • Unmount all partitions: umount /dev/sdX*\n\
                 • Check what's using it: lsof /dev/sdX\n\
                 • Kill processes using the device\n\
                 • Try running with --force",
                base_msg
            )
        }
        19 => {
            // ENODEV - No such device
            format!(
                "Device disconnected: {}\n\
                 \n\
                 The device is no longer available.\n\
                 \n\
                 Suggestions:\n\
                 • Check that the device is still connected\n\
                 • Try reconnecting the device\n\
                 • Use a different USB port",
                base_msg
            )
        }
        28 => {
            // ENOSPC - No space left on device
            format!(
                "No space left on device: {}\n\
                 \n\
                 The device is full or too small for the image.\n\
                 \n\
                 Suggestions:\n\
                 • Use a larger device\n\
                 • Verify the image size matches the device capacity\n\
                 • Check for hidden partitions using: fdisk -l",
                base_msg
            )
        }
        30 => {
            // EROFS - Read-only file system
            format!(
                "Read-only device: {}\n\
                 \n\
                 The device is mounted read-only or write-protected.\n\
                 \n\
                 Suggestions:\n\
                 • Check if the device has a physical write-protect switch\n\
                 • Unmount and remount with write permissions\n\
                 • Some SD cards have a lock slider on the side",
                base_msg
            )
        }
        _ => {
            format!(
                "System error (errno {}): {}\n\
                 \n\
                 Suggestions:\n\
                 • Check dmesg for detailed error messages:\n\
                   dmesg | tail -50\n\
                 • Run with --resume to continue from the last checkpoint",
                errno, base_msg
            )
        }
    }
}

/// Format Windows-specific error codes
#[cfg(windows)]
fn format_windows_error(code: i32, base_msg: &str) -> String {
    match code {
        5 => {
            // ERROR_ACCESS_DENIED
            format!(
                "Access denied: {}\n\
                 \n\
                 Suggestions:\n\
                 • Run as Administrator\n\
                 • Close any programs accessing the device\n\
                 • Check if another backup or sync program is running",
                base_msg
            )
        }
        21 => {
            // ERROR_NOT_READY
            format!(
                "Device not ready: {}\n\
                 \n\
                 Suggestions:\n\
                 • Wait for the device to initialize\n\
                 • Try reconnecting the device\n\
                 • Check Device Manager for issues",
                base_msg
            )
        }
        32 => {
            // ERROR_SHARING_VIOLATION
            format!(
                "Sharing violation: {}\n\
                 \n\
                 Another program is using the device.\n\
                 \n\
                 Suggestions:\n\
                 • Close File Explorer windows showing the drive\n\
                 • Close any antivirus scanning the device\n\
                 • Use Resource Monitor to find the program",
                base_msg
            )
        }
        112 => {
            // ERROR_DISK_FULL
            format!(
                "Disk full: {}\n\
                 \n\
                 Suggestions:\n\
                 • Use a larger device\n\
                 • Verify the image size fits on the device",
                base_msg
            )
        }
        1117 => {
            // ERROR_IO_DEVICE
            format!(
                "I/O device error: {}\n\
                 \n\
                 A hardware error occurred.\n\
                 \n\
                 Suggestions:\n\
                 • Try a different USB port\n\
                 • Use a different device\n\
                 • Check Event Viewer for details",
                base_msg
            )
        }
        _ => {
            format!(
                "Windows error (code {}): {}\n\
                 \n\
                 Suggestions:\n\
                 • Check Event Viewer for details\n\
                 • Run with --resume to continue from the last checkpoint",
                code, base_msg
            )
        }
    }
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

/// Display partition information for the source image
#[cfg(feature = "partition-info")]
fn display_source_partitions(source_path: &str, silent: bool) -> Result<()> {
    use engraver_core::partition::{format_offset, format_size as part_format_size};

    println_if!(silent, "\n{}", style("Source Partitions:").bold());

    // Read partition header from source
    let buffer = match read_partition_header(source_path) {
        Ok(buf) => buf,
        Err(e) => {
            println_if!(
                silent,
                "  {} Could not read partition table: {}",
                style("ℹ").blue(),
                e
            );
            return Ok(());
        }
    };

    // Parse partition table
    let info = match inspect_from_buffer(&buffer) {
        Ok(info) => info,
        Err(e) => {
            println_if!(
                silent,
                "  {} Could not parse partition table: {}",
                style("ℹ").blue(),
                e
            );
            return Ok(());
        }
    };

    // Display based on partition table type
    match info.table_type {
        PartitionTableType::None => {
            println_if!(
                silent,
                "  {} No partition table detected (may be ISO 9660 or raw filesystem)",
                style("ℹ").blue()
            );
        }
        table_type => {
            // Print table header
            println_if!(
                silent,
                "┌─────────────────────────────────────────────────────────────────┐"
            );
            println_if!(
                silent,
                "│ {} Partition Table{}│",
                table_type,
                " ".repeat(53 - table_type.to_string().len())
            );

            if let Some(ref disk_id) = info.disk_id {
                let id_str = format!("Disk ID: {}", disk_id);
                let padding = 64usize.saturating_sub(id_str.len());
                println_if!(silent, "│ {}{}│", id_str, " ".repeat(padding));
            }

            println_if!(
                silent,
                "├───────┬──────────────┬──────────────┬───────────────────────────┤"
            );
            println_if!(
                silent,
                "│  #    │ Start        │ Size         │ Type / Name               │"
            );
            println_if!(
                silent,
                "├───────┼──────────────┼──────────────┼───────────────────────────┤"
            );

            // Print each partition
            for part in &info.partitions {
                let boot_marker = if part.bootable { "*" } else { " " };
                let start = format_offset(part.start_offset);
                let size = part_format_size(part.size);

                // Combine type and name for display
                let type_name = if let Some(ref name) = part.name {
                    if name.is_empty() {
                        part.partition_type.clone()
                    } else {
                        format!("{} ({})", part.partition_type, name)
                    }
                } else {
                    part.partition_type.clone()
                };

                // Truncate if too long
                let type_name_display = if type_name.len() > 25 {
                    format!("{}...", &type_name[..22])
                } else {
                    type_name
                };

                println_if!(
                    silent,
                    "│ {:>2} {} │ {:>12} │ {:>12} │ {:25} │",
                    part.number,
                    boot_marker,
                    start,
                    size,
                    type_name_display
                );
            }

            println_if!(
                silent,
                "└───────┴──────────────┴──────────────┴───────────────────────────┘"
            );

            // Show bootable marker legend if any partitions are bootable
            if info.partitions.iter().any(|p| p.bootable) {
                println_if!(
                    silent,
                    "  {} = bootable/active partition",
                    style("*").bold()
                );
            }
        }
    }

    Ok(())
}

/// Stub for when partition-info feature is disabled
#[cfg(not(feature = "partition-info"))]
fn display_source_partitions(_source_path: &str, silent: bool) -> Result<()> {
    println_if!(
        silent,
        "  {} Partition inspection not available (compiled without partition-info feature)",
        style("ℹ").blue()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
            auto_checksum: false,
            show_partitions: false,
        };

        assert_eq!(args.source, "ubuntu.iso");
        assert_eq!(args.target, "/dev/sdb");
        assert!(args.verify);
        assert!(!args.skip_confirm);
        assert_eq!(args.block_size, "4M");
        assert!(args.checksum.is_some());
        assert!(!args.force);
        assert!(!args.cancel_flag.load(Ordering::Relaxed));
        assert!(!args.auto_checksum);
        assert!(!args.show_partitions);
    }

    #[test]
    fn test_write_args_with_show_partitions() {
        let args = WriteArgs {
            source: "debian.img".to_string(),
            target: "/dev/sdc".to_string(),
            verify: false,
            skip_confirm: true,
            block_size: "1M".to_string(),
            checksum: None,
            checksum_algo: "sha256".to_string(),
            force: false,
            no_unmount: true,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            silent: true,
            resume: false,
            checkpoint: false,
            auto_checksum: true,
            show_partitions: true,
        };

        assert_eq!(args.source, "debian.img");
        assert_eq!(args.target, "/dev/sdc");
        assert!(!args.verify);
        assert!(args.skip_confirm);
        assert_eq!(args.block_size, "1M");
        assert!(args.checksum.is_none());
        assert!(!args.force);
        assert!(args.no_unmount);
        assert!(args.silent);
        assert!(!args.resume);
        assert!(!args.checkpoint);
        assert!(args.auto_checksum);
        assert!(args.show_partitions);
    }

    #[test]
    fn test_write_args_all_flags_enabled() {
        let args = WriteArgs {
            source: "image.iso".to_string(),
            target: "/dev/sdd".to_string(),
            verify: true,
            skip_confirm: true,
            block_size: "8M".to_string(),
            checksum: Some("deadbeef".to_string()),
            checksum_algo: "md5".to_string(),
            force: true,
            no_unmount: true,
            cancel_flag: Arc::new(AtomicBool::new(true)),
            silent: true,
            resume: true,
            checkpoint: true,
            auto_checksum: true,
            show_partitions: true,
        };

        assert!(args.verify);
        assert!(args.skip_confirm);
        assert!(args.force);
        assert!(args.no_unmount);
        assert!(args.cancel_flag.load(Ordering::Relaxed));
        assert!(args.silent);
        assert!(args.resume);
        assert!(args.checkpoint);
        assert!(args.auto_checksum);
        assert!(args.show_partitions);
    }

    // -------------------------------------------------------------------------
    // Capacity check tests
    // -------------------------------------------------------------------------

    const MIB: u64 = 1024 * 1024;
    const GIB: u64 = 1024 * MIB;

    fn compressed_source(size_on_disk: u64) -> engraver_core::SourceInfo {
        engraver_core::SourceInfo::compressed("image.img.gz", size_on_disk, SourceType::Gzip)
    }

    fn removable_drive(size: u64) -> Drive {
        Drive {
            path: "/dev/sdz".to_string(),
            raw_path: "/dev/sdz".to_string(),
            size,
            removable: true,
            ..Default::default()
        }
    }

    #[test]
    fn test_capacity_of_a_compressed_source_is_unknown_when_the_file_is_small() {
        // A 1 MiB file can decompress to far more than a 4 GiB drive holds
        assert_eq!(
            capacity(&compressed_source(MIB), 4 * GIB),
            Capacity::Unknown
        );
    }

    #[test]
    fn test_capacity_of_a_compressed_source_larger_than_the_drive_is_too_large() {
        // Refused before the drive is touched, not by ENOSPC partway through
        assert_eq!(
            capacity(&compressed_source(8 * GIB), 4 * GIB),
            Capacity::CompressedTooLarge(8 * GIB)
        );
    }

    #[test]
    fn test_capacity_of_a_plain_source_is_judged_by_its_length() {
        let source = |size| engraver_core::SourceInfo::local("image.iso", size);

        assert_eq!(capacity(&source(GIB), 4 * GIB), Capacity::Fits);
        assert_eq!(capacity(&source(4 * GIB), 4 * GIB), Capacity::Fits);
        assert_eq!(
            capacity(&source(4 * GIB + 1), 4 * GIB),
            Capacity::TooLarge(4 * GIB + 1)
        );
    }

    #[test]
    fn test_validate_target_device_accepts_a_compressed_source_of_unknown_image_size() {
        let drives = vec![removable_drive(4 * GIB)];

        let drive = validate_target_device(
            "/dev/sdz",
            &drives,
            false,
            true,
            &compressed_source(MIB),
            true,
        )
        .expect("the capacity check must not judge an unknown image by its file size");

        assert_eq!(drive.path, "/dev/sdz");
    }

    #[test]
    fn test_validate_target_device_refuses_an_image_larger_than_the_drive() {
        let drives = vec![removable_drive(4 * GIB)];
        let source = engraver_core::SourceInfo::local("image.iso", 8 * GIB);

        let err = validate_target_device("/dev/sdz", &drives, false, true, &source, true)
            .expect_err("an 8 GiB image does not fit on a 4 GiB drive");

        assert!(err.to_string().contains("larger than target"), "{err}");
    }

    // -------------------------------------------------------------------------
    // setup_checksum cancellation tests
    // -------------------------------------------------------------------------

    const SHA256_OF_64K_OF_0XAB: &str =
        "7c56cd2bee665a1839e41377e70c4a00e688c2b31e6e25638185b5ad1b1537e1";

    fn checksum_args(expected: &str, cancel: bool) -> (tempfile::NamedTempFile, WriteArgs) {
        let mut source = tempfile::NamedTempFile::new().unwrap();
        source.write_all(&[0xABu8; 64 * 1024]).unwrap();
        let args = WriteArgs {
            source: source.path().to_str().unwrap().to_string(),
            target: "/dev/null".to_string(),
            verify: false,
            skip_confirm: true,
            block_size: "4K".to_string(),
            checksum: Some(expected.to_string()),
            checksum_algo: "sha256".to_string(),
            force: false,
            no_unmount: true,
            cancel_flag: Arc::new(AtomicBool::new(cancel)),
            silent: true,
            resume: false,
            checkpoint: false,
            auto_checksum: false,
            show_partitions: false,
        };
        (source, args)
    }

    fn silent_ctx() -> WriteContext {
        WriteContext {
            silent: true,
            block_size: 4096,
        }
    }

    #[test]
    fn test_setup_checksum_with_pending_cancel_reports_cancelled() {
        let (_source, args) = checksum_args(SHA256_OF_64K_OF_0XAB, true);

        let err = setup_checksum(&args, Some(64 * 1024), &silent_ctx())
            .expect_err("a pending cancel must stop the checksum step");

        let message = err.to_string();
        assert!(
            message.contains("Cancelled before writing"),
            "unexpected message: {message}"
        );
        assert!(
            !message.contains("Checksum verification failed"),
            "a cancel is not a checksum failure: {message}"
        );
        assert!(
            args.cancel_flag.load(Ordering::SeqCst),
            "the cancel must stay set so the write that follows also stops"
        );
    }

    #[test]
    fn test_setup_checksum_with_pending_cancel_returns_a_cancelled_error() {
        let (_source, args) = checksum_args(SHA256_OF_64K_OF_0XAB, true);

        let err = setup_checksum(&args, Some(64 * 1024), &silent_ctx())
            .expect_err("a pending cancel must stop the checksum step");

        crate::commands::assert_cancelled(&err);
    }

    #[test]
    fn test_setup_checksum_still_reports_a_real_mismatch() {
        let (_source, args) = checksum_args(&"00".repeat(32), false);

        let err = setup_checksum(&args, Some(64 * 1024), &silent_ctx())
            .expect_err("a wrong checksum must fail");

        assert!(
            err.to_string().contains("Checksum verification failed"),
            "unexpected message: {err}"
        );
    }

    #[test]
    fn test_setup_checksum_passes_for_matching_checksum() {
        let (_source, args) = checksum_args(SHA256_OF_64K_OF_0XAB, false);

        setup_checksum(&args, Some(64 * 1024), &silent_ctx())
            .expect("a matching checksum must pass");
    }
}
