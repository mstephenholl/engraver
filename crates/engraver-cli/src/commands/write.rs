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
    validate_source, ChecksumAlgorithm, Source, SourceType, Verifier, VerifyConfig,
    WriteConfig, Writer,
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
}

/// Execute the write command
pub fn execute(args: WriteArgs) -> Result<()> {
    // Parse block size
    let block_size = parse_block_size(&args.block_size)?;

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
    println!(
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
        println!(
            "  {} ({}, {})",
            style("✓").green(),
            format_size(size),
            source_type_str
        );
    } else {
        println!("  {} (size unknown, {})", style("✓").green(), source_type_str);
    }

    // Step 2: Validate target device
    println!(
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

    println!(
        "  {} {} ({})",
        style("✓").green(),
        target_drive.display_name(),
        format_size(target_drive.size)
    );

    // Show mount points that will be unmounted
    if !target_drive.mount_points.is_empty() && !args.no_unmount {
        println!(
            "  {} Will unmount: {}",
            style("⚠").yellow(),
            target_drive.mount_points.join(", ")
        );
    }

    // Step 3: Confirmation
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
                .last()
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
        println!("\n{}", style("Unmounting device...").bold());

        // Use the device path for unmounting (unmount all partitions)
        match unmount_device(&target_drive.path) {
            Ok(()) => println!("  {} Device unmounted", style("✓").green()),
            Err(e) => {
                // Some platforms may return error if nothing to unmount
                tracing::debug!("Unmount result: {}", e);
                println!("  {} Unmount: {}", style("ℹ").blue(), e);
            }
        }
    }

    // Step 5: Verify source checksum (if provided)
    if let Some(expected_checksum) = &args.checksum {
        println!("\n{}", style("Verifying source checksum...").bold());

        let algo: ChecksumAlgorithm = args
            .checksum_algo
            .parse()
            .with_context(|| format!("Invalid checksum algorithm: {}", args.checksum_algo))?;

        let mut source_for_checksum =
            Source::open(&args.source).context("Failed to open source for checksum")?;

        let pb = create_progress_bar(source_size, "Checksumming");

        let config = VerifyConfig::new().block_size(block_size);
        let pb_clone = pb.clone();
        let mut verifier = Verifier::with_config(config).on_progress(move |p| {
            pb_clone.set_position(p.bytes_processed);
        });

        let result =
            verifier.verify_checksum(&mut source_for_checksum, algo, expected_checksum, source_size);

        pb.finish_and_clear();

        match result {
            Ok(_) => println!(
                "  {} Checksum verified ({})",
                style("✓").green(),
                algo.name()
            ),
            Err(e) => bail!("Checksum verification failed: {}", e),
        }
    }

    // Step 6: Open source and target device
    println!("\n{}", style("Writing image...").bold());

    let mut source = Source::open(&args.source).context("Failed to open source")?;

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

    // Step 7: Write with progress
    let total_size = source_size.unwrap_or(0);
    let pb = create_write_progress_bar(total_size);

    let cancel_flag = args.cancel_flag.clone();

    let config = WriteConfig::new()
        .block_size(block_size)
        .sync_each_block(false)
        .sync_on_complete(true);

    let writer = Writer::with_config(config);

    // Set up progress callback
    let pb_clone = pb.clone();
    let writer = writer.on_progress(move |progress| {
        pb_clone.set_position(progress.bytes_written);
        pb_clone.set_message(format!(
            "{}/s, ETA: {}",
            format_size(progress.speed_bps),
            progress.eta_display()
        ));
    });

    // Connect cancel flag
    let writer_cancel = writer.cancel_handle();
    std::thread::spawn(move || {
        while cancel_flag.load(Ordering::SeqCst) {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        writer_cancel.store(true, Ordering::SeqCst);
    });

    let mut writer = writer;
    let start_time = Instant::now();

    let write_result = writer.write(&mut source, &mut *target, total_size);

    pb.finish_and_clear();

    match write_result {
        Ok(result) => {
            let elapsed = start_time.elapsed();
            let speed = if elapsed.as_secs_f64() > 0.0 {
                result.bytes_written as f64 / elapsed.as_secs_f64()
            } else {
                0.0
            };

            println!(
                "  {} Wrote {} in {:.1}s ({}/s)",
                style("✓").green(),
                format_size(result.bytes_written),
                elapsed.as_secs_f64(),
                format_size(speed as u64)
            );
        }
        Err(engraver_core::Error::Cancelled) => {
            println!("\n{}", style("Write cancelled by user.").yellow());
            return Ok(());
        }
        Err(e) => {
            bail!("Write failed: {}", e);
        }
    }

    // Step 8: Sync using platform layer
    print!("  Syncing... ");
    std::io::stdout().flush()?;
    target
        .sync()
        .context("Failed to sync device")?;
    println!("{}", style("done").green());

    // Step 9: Verify (if requested)
    if args.verify {
        println!("\n{}", style("Verifying write...").bold());

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

            let pb = create_progress_bar(source_size, "Verifying");

            let config = VerifyConfig::new().block_size(block_size);
            let pb_clone = pb.clone();
            let mut verifier = Verifier::with_config(config).on_progress(move |p| {
                pb_clone.set_position(p.bytes_processed);
            });

            let verify_result = verifier.compare(&mut source_file, &mut *target, total_size);

            pb.finish_and_clear();

            match verify_result {
                Ok(result) if result.success => {
                    println!(
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
            println!(
                "  {} Source is remote/compressed, using checksum verification",
                style("ℹ").blue()
            );

            // Calculate checksum of what we wrote
            target.seek(SeekFrom::Start(0))?;

            let pb = create_progress_bar(Some(total_size), "Checksumming");

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
            println!("  Calculating source checksum...");
            let mut source_for_checksum =
                Source::open(&args.source).context("Failed to reopen source")?;

            let pb = create_progress_bar(source_size, "Checksumming source");

            let config = VerifyConfig::new().block_size(block_size);
            let pb_clone = pb.clone();
            let mut verifier = Verifier::with_config(config).on_progress(move |p| {
                pb_clone.set_position(p.bytes_processed);
            });

            let source_checksum = verifier
                .calculate_checksum(&mut source_for_checksum, ChecksumAlgorithm::Sha256, source_size)
                .context("Failed to checksum source")?;

            pb.finish_and_clear();

            if written_checksum.matches(&source_checksum) {
                println!(
                    "  {} Checksum verification passed",
                    style("✓").green()
                );
                println!("    SHA-256: {}", written_checksum.to_hex());
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
    println!();
    println!(
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
fn create_progress_bar(total: Option<u64>, operation: &str) -> ProgressBar {
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
fn create_write_progress_bar(total: u64) -> ProgressBar {
    let pb = if total > 0 {
        ProgressBar::new(total)
    } else {
        ProgressBar::new_spinner()
    };

    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "  {spinner:.green} Writing [{bar:40.cyan/blue}] {bytes}/{total_bytes} {msg}",
            )
            .unwrap()
            .progress_chars("█▓░"),
    );

    pb
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_parse_block_size_invalid() {
        assert!(parse_block_size("100").is_err()); // Too small
        assert!(parse_block_size("128M").is_err()); // Too large
        assert!(parse_block_size("abc").is_err()); // Invalid
    }

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
    fn test_get_raw_device_path() {
        #[cfg(target_os = "macos")]
        {
            assert_eq!(get_raw_device_path("/dev/disk2"), "/dev/rdisk2");
            assert_eq!(get_raw_device_path("/dev/rdisk2"), "/dev/rdisk2");
        }

        #[cfg(target_os = "linux")]
        {
            assert_eq!(get_raw_device_path("/dev/sdb"), "/dev/sdb");
        }
    }
}
