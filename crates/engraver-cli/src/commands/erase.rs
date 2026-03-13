//! Erase command - zero-fills a drive
//!
//! This command completely wipes a drive by writing zeros across the entire
//! device capacity. Useful for securely wiping drives before repurposing or
//! disposing of them.

use anyhow::{bail, Context, Result};
use console::style;
use dialoguer::Confirm;
use indicatif::{ProgressBar, ProgressStyle};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use engraver_detect::{list_drives, Drive};
use engraver_platform::{has_elevated_privileges, open_device, unmount_device, OpenOptions};

/// Arguments for the erase command
pub struct EraseArgs {
    pub target: String,
    pub skip_confirm: bool,
    pub block_size: String,
    pub force: bool,
    pub no_unmount: bool,
    pub cancel_flag: Arc<AtomicBool>,
    pub silent: bool,
}

/// Execute the erase command
pub fn execute(args: EraseArgs) -> Result<()> {
    let block_size = parse_block_size(&args.block_size)?;
    let silent = args.silent;

    // Step 1: Check for elevated privileges
    if !has_elevated_privileges() {
        #[cfg(unix)]
        bail!(
            "Root privileges required.\n\
             Try running with: sudo engraver erase ..."
        );

        #[cfg(windows)]
        bail!(
            "Administrator privileges required.\n\
             Right-click and select 'Run as administrator'."
        );

        #[cfg(not(any(unix, windows)))]
        bail!("Elevated privileges required for raw device access.");
    }

    // Step 2: Validate target device
    println_if!(
        silent,
        "{} {}",
        style("Target:").bold(),
        style(&args.target).cyan()
    );

    let drives = list_drives().context("Failed to list drives")?;
    let target_drive = find_drive(&drives, &args.target)?;

    // Safety check: refuse system drives
    if target_drive.is_system && !args.force {
        bail!(
            "Refusing to erase system drive: {}\n\
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

    println_if!(
        silent,
        "  {} {} ({})",
        style("✓").green(),
        target_drive.display_name(),
        format_size(target_drive.size)
    );

    // Show mount points that will be unmounted
    if !target_drive.mount_points.is_empty() && !args.no_unmount {
        println_if!(
            silent,
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
            style("║  The entire device will be zero-filled.                    ║")
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
            "Erase {} ({})?",
            target_drive.path,
            format_size(target_drive.size)
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

    // Step 4: Unmount device
    if !args.no_unmount {
        println_if!(silent, "\n{}", style("Unmounting device...").bold());

        match unmount_device(&target_drive.path) {
            Ok(()) => println_if!(silent, "  {} Device unmounted", style("✓").green()),
            Err(e) => {
                tracing::debug!("Unmount result: {}", e);
                println_if!(silent, "  {} Unmount: {}", style("ℹ").blue(), e);
            }
        }
    }

    // Step 5: Open device with O_DIRECT
    let total_size = target_drive.size;
    let total_blocks = total_size.div_ceil(block_size as u64);
    println_if!(silent, "\n{}", style("Erasing device...").bold());
    println_if!(
        silent,
        "  {} Block size: {}, Total blocks: {}",
        style("ℹ").blue(),
        format_size(block_size as u64),
        total_blocks
    );

    let device_path = get_raw_device_path(&target_drive.path);
    let options = OpenOptions::new()
        .write(true)
        .direct_io(true)
        .block_size(block_size);

    let mut target = open_device(&device_path, options)
        .with_context(|| format!("Failed to open device: {}", device_path))?;

    // Step 6: Write zeros block-by-block
    let zero_buf = vec![0u8; block_size];
    let pb = create_erase_progress_bar(total_size, silent);
    let cancel_flag = args.cancel_flag.clone();
    let start_time = Instant::now();
    let mut bytes_written: u64 = 0;

    loop {
        // Check cancellation
        if !cancel_flag.load(Ordering::SeqCst) {
            pb.finish_and_clear();
            // Sync to flush any pending writes before returning
            if let Err(e) = target.sync() {
                tracing::debug!("Sync after cancel: {}", e);
            }
            println_if!(silent, "\n{}", style("Erase cancelled by user.").yellow());
            return Ok(());
        }

        let remaining = total_size - bytes_written;
        if remaining == 0 {
            break;
        }

        let write_len = std::cmp::min(remaining, block_size as u64) as usize;
        match target.write(&zero_buf[..write_len]) {
            Ok(n) => {
                bytes_written += n as u64;
                pb.set_position(bytes_written);

                // Update progress message
                let elapsed = start_time.elapsed().as_secs_f64();
                let speed = if elapsed > 0.0 {
                    bytes_written as f64 / elapsed
                } else {
                    0.0
                };
                let remaining_bytes = total_size - bytes_written;
                let eta = if speed > 0.0 {
                    remaining_bytes as f64 / speed
                } else {
                    0.0
                };

                let blocks_done = bytes_written.div_ceil(block_size as u64);
                pb.set_message(format!(
                    "{}/s | Block {}/{} | ETA: {}",
                    format_size(speed as u64),
                    blocks_done,
                    total_blocks,
                    format_eta(eta)
                ));
            }
            Err(e) => {
                pb.finish_and_clear();
                bail!("Write error at byte {}: {}", bytes_written, e);
            }
        }
    }

    pb.finish_and_clear();

    let elapsed = start_time.elapsed();
    let speed = if elapsed.as_secs_f64() > 0.0 {
        bytes_written as f64 / elapsed.as_secs_f64()
    } else {
        0.0
    };

    println_if!(
        silent,
        "  {} Erased {} ({} blocks) in {:.1}s ({}/s)",
        style("✓").green(),
        format_size(bytes_written),
        total_blocks,
        elapsed.as_secs_f64(),
        format_size(speed as u64)
    );

    // Step 7: Sync
    print_if!(silent, "  Syncing... ");
    if !silent {
        std::io::stdout().flush()?;
    }
    target.sync().context("Failed to sync device")?;
    println_if!(silent, "{}", style("done").green());

    // Done
    println_if!(silent);
    println_if!(
        silent,
        "{}",
        style(
            "✓ Erase complete! The device has been zero-filled. You can safely remove the drive."
        )
        .green()
        .bold()
    );

    Ok(())
}

/// Find a drive by path
fn find_drive<'a>(drives: &'a [Drive], path: &str) -> Result<&'a Drive> {
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

/// Format ETA duration
fn format_eta(secs: f64) -> String {
    let secs = secs as u64;
    if secs > 3600 {
        format!("{}h {}m {}s", secs / 3600, (secs % 3600) / 60, secs % 60)
    } else if secs > 60 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}s", secs)
    }
}

/// Create a progress bar for the erase operation
fn create_erase_progress_bar(total: u64, silent: bool) -> ProgressBar {
    if silent {
        return ProgressBar::hidden();
    }

    let pb = ProgressBar::new(total);

    pb.set_style(
        ProgressStyle::default_bar()
            .template("  {spinner:.green} Erasing [{bar:40.cyan/blue}] {bytes}/{total_bytes} {msg}")
            .unwrap()
            .progress_chars("█▓░"),
    );

    pb
}

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================================
    // Helper to create a test Drive
    // =========================================================================

    fn make_drive(path: &str) -> Drive {
        Drive {
            path: path.to_string(),
            raw_path: path.to_string(),
            name: "Test Drive".to_string(),
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
        }
    }

    // =========================================================================
    // parse_block_size tests
    // =========================================================================

    #[test]
    fn test_parse_block_size_bytes() {
        assert_eq!(parse_block_size("4096").unwrap(), 4096);
        assert_eq!(parse_block_size("8192").unwrap(), 8192);
        assert_eq!(parse_block_size("65536").unwrap(), 65536);
    }

    #[test]
    fn test_parse_block_size_kilobytes() {
        assert_eq!(parse_block_size("4K").unwrap(), 4096);
        assert_eq!(parse_block_size("4k").unwrap(), 4096);
        assert_eq!(parse_block_size("8K").unwrap(), 8192);
        assert_eq!(parse_block_size("64K").unwrap(), 64 * 1024);
        assert_eq!(parse_block_size("512K").unwrap(), 512 * 1024);
    }

    #[test]
    fn test_parse_block_size_megabytes() {
        assert_eq!(parse_block_size("1M").unwrap(), 1024 * 1024);
        assert_eq!(parse_block_size("4M").unwrap(), 4 * 1024 * 1024);
        assert_eq!(parse_block_size("16M").unwrap(), 16 * 1024 * 1024);
        assert_eq!(parse_block_size("64M").unwrap(), 64 * 1024 * 1024);
    }

    #[test]
    fn test_parse_block_size_gigabytes() {
        // 1G is within the 64M max, so this should fail
        assert!(parse_block_size("1G").is_err());
    }

    #[test]
    fn test_parse_block_size_whitespace_trimming() {
        assert_eq!(parse_block_size("  4K  ").unwrap(), 4096);
        assert_eq!(parse_block_size(" 1M ").unwrap(), 1024 * 1024);
        assert_eq!(parse_block_size("\t4096\t").unwrap(), 4096);
    }

    #[test]
    fn test_parse_block_size_case_insensitive() {
        assert_eq!(parse_block_size("4k").unwrap(), 4096);
        assert_eq!(parse_block_size("4K").unwrap(), 4096);
        assert_eq!(parse_block_size("1m").unwrap(), 1024 * 1024);
        assert_eq!(parse_block_size("1M").unwrap(), 1024 * 1024);
    }

    #[test]
    fn test_parse_block_size_exact_minimum() {
        // Exactly 4096 bytes (4K) is the minimum
        assert_eq!(parse_block_size("4K").unwrap(), 4096);
        assert_eq!(parse_block_size("4096").unwrap(), 4096);
    }

    #[test]
    fn test_parse_block_size_below_minimum() {
        assert!(parse_block_size("1K").is_err());
        assert!(parse_block_size("2K").is_err());
        assert!(parse_block_size("100").is_err());
        assert!(parse_block_size("1").is_err());
        assert!(parse_block_size("0").is_err());
    }

    #[test]
    fn test_parse_block_size_exact_maximum() {
        // Exactly 64M is the maximum
        assert_eq!(parse_block_size("64M").unwrap(), 64 * 1024 * 1024);
    }

    #[test]
    fn test_parse_block_size_above_maximum() {
        assert!(parse_block_size("65M").is_err());
        assert!(parse_block_size("128M").is_err());
        assert!(parse_block_size("1G").is_err());
    }

    #[test]
    fn test_parse_block_size_non_numeric() {
        assert!(parse_block_size("abc").is_err());
        assert!(parse_block_size("K").is_err());
        assert!(parse_block_size("M").is_err());
        assert!(parse_block_size("xyzK").is_err());
    }

    #[test]
    fn test_parse_block_size_empty_string() {
        // Empty string after trim: parsing "" as number should fail
        assert!(parse_block_size("").is_err());
    }

    #[test]
    fn test_parse_block_size_error_messages() {
        let err = parse_block_size("100").unwrap_err();
        assert!(
            err.to_string().contains("at least 4K"),
            "Expected 'at least 4K' in: {}",
            err
        );

        let err = parse_block_size("128M").unwrap_err();
        assert!(
            err.to_string().contains("at most 64M"),
            "Expected 'at most 64M' in: {}",
            err
        );

        let err = parse_block_size("abc").unwrap_err();
        assert!(
            err.to_string().contains("Invalid block size"),
            "Expected 'Invalid block size' in: {}",
            err
        );
    }

    // =========================================================================
    // format_size tests
    // =========================================================================

    #[test]
    fn test_format_size_zero() {
        assert_eq!(format_size(0), "0 B");
    }

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(1), "1 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1023), "1023 B");
    }

    #[test]
    fn test_format_size_kilobytes() {
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1536), "1.50 KB");
        assert_eq!(format_size(10 * 1024), "10.00 KB");
    }

    #[test]
    fn test_format_size_megabytes() {
        assert_eq!(format_size(1024 * 1024), "1.00 MB");
        assert_eq!(format_size(500 * 1024 * 1024), "500.00 MB");
    }

    #[test]
    fn test_format_size_gigabytes() {
        assert_eq!(format_size(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(format_size(32u64 * 1024 * 1024 * 1024), "32.00 GB");
    }

    #[test]
    fn test_format_size_terabytes() {
        assert_eq!(format_size(1024u64 * 1024 * 1024 * 1024), "1.00 TB");
        assert_eq!(format_size(2u64 * 1024 * 1024 * 1024 * 1024), "2.00 TB");
    }

    #[test]
    fn test_format_size_realistic_drive_sizes() {
        // A "16 GB" USB drive often reports ~15.x GB in binary
        assert_eq!(format_size(16_000_000_000), "14.90 GB");
        // A 1 TB drive
        assert_eq!(format_size(1_000_000_000_000), "931.32 GB");
    }

    #[test]
    fn test_format_size_boundary_values() {
        // Just under 1 KB
        assert_eq!(format_size(1023), "1023 B");
        // Exactly 1 KB
        assert_eq!(format_size(1024), "1.00 KB");
        // Just under 1 MB
        let just_under_mb = 1024 * 1024 - 1;
        let result = format_size(just_under_mb);
        assert!(result.ends_with("KB"), "Expected KB, got: {}", result);
        // Exactly 1 MB
        assert_eq!(format_size(1024 * 1024), "1.00 MB");
        // Exactly 1 GB
        assert_eq!(format_size(1024 * 1024 * 1024), "1.00 GB");
    }

    // =========================================================================
    // format_eta tests
    // =========================================================================

    #[test]
    fn test_format_eta_seconds_only() {
        assert_eq!(format_eta(0.0), "0s");
        assert_eq!(format_eta(1.0), "1s");
        assert_eq!(format_eta(30.0), "30s");
        assert_eq!(format_eta(59.0), "59s");
    }

    #[test]
    fn test_format_eta_fractional_rounds_down() {
        // f64 → u64 truncates
        assert_eq!(format_eta(0.9), "0s");
        assert_eq!(format_eta(59.9), "59s");
    }

    #[test]
    fn test_format_eta_minutes_boundary() {
        // format_eta uses `> 60` not `>= 60`, so exactly 60 stays in seconds format
        assert_eq!(format_eta(60.0), "60s");
        assert_eq!(format_eta(61.0), "1m 1s");
    }

    #[test]
    fn test_format_eta_minutes() {
        assert_eq!(format_eta(90.0), "1m 30s");
        assert_eq!(format_eta(120.0), "2m 0s");
        assert_eq!(format_eta(3599.0), "59m 59s");
    }

    #[test]
    fn test_format_eta_hours_boundary() {
        // format_eta uses `> 3600` not `>= 3600`, so exactly 3600 stays in minutes format
        assert_eq!(format_eta(3600.0), "60m 0s");
        assert_eq!(format_eta(3601.0), "1h 0m 1s");
    }

    #[test]
    fn test_format_eta_hours() {
        assert_eq!(format_eta(3661.0), "1h 1m 1s");
        assert_eq!(format_eta(7200.0), "2h 0m 0s");
        assert_eq!(format_eta(7325.0), "2h 2m 5s");
    }

    #[test]
    fn test_format_eta_large_values() {
        // 24 hours
        assert_eq!(format_eta(86400.0), "24h 0m 0s");
    }

    // =========================================================================
    // get_raw_device_path tests
    // =========================================================================

    #[test]
    fn test_get_raw_device_path_macos() {
        #[cfg(target_os = "macos")]
        {
            // Standard disk path should be converted to rdisk
            assert_eq!(get_raw_device_path("/dev/disk2"), "/dev/rdisk2");
            assert_eq!(get_raw_device_path("/dev/disk0"), "/dev/rdisk0");
            assert_eq!(get_raw_device_path("/dev/disk10"), "/dev/rdisk10");

            // Already rdisk should be unchanged
            assert_eq!(get_raw_device_path("/dev/rdisk2"), "/dev/rdisk2");
            assert_eq!(get_raw_device_path("/dev/rdisk0"), "/dev/rdisk0");

            // Non-disk paths should be unchanged
            assert_eq!(get_raw_device_path("/dev/sdb"), "/dev/sdb");
            assert_eq!(get_raw_device_path("/some/path"), "/some/path");
        }
    }

    #[test]
    fn test_get_raw_device_path_linux() {
        #[cfg(target_os = "linux")]
        {
            // Linux paths should be passed through unchanged
            assert_eq!(get_raw_device_path("/dev/sdb"), "/dev/sdb");
            assert_eq!(get_raw_device_path("/dev/sda"), "/dev/sda");
            assert_eq!(get_raw_device_path("/dev/nvme0n1"), "/dev/nvme0n1");
            assert_eq!(get_raw_device_path("/dev/mmcblk0"), "/dev/mmcblk0");
        }
    }

    #[test]
    fn test_get_raw_device_path_passthrough() {
        // On any platform, arbitrary paths should pass through
        // (macOS only transforms /dev/disk* patterns)
        let result = get_raw_device_path("/some/other/path");
        assert_eq!(result, "/some/other/path");
    }

    // =========================================================================
    // find_drive tests
    // =========================================================================

    #[test]
    fn test_find_drive_by_path() {
        let drives = vec![make_drive("/dev/sdb")];

        let result = find_drive(&drives, "/dev/sdb");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().path, "/dev/sdb");
    }

    #[test]
    fn test_find_drive_by_raw_path() {
        let mut drive = make_drive("/dev/disk2");
        drive.raw_path = "/dev/rdisk2".to_string();
        let drives = vec![drive];

        // Matching by raw_path
        let result = find_drive(&drives, "/dev/rdisk2");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().path, "/dev/disk2");
    }

    #[test]
    fn test_find_drive_by_normalized_path() {
        #[cfg(target_os = "macos")]
        {
            // On macOS, find_drive normalizes /dev/disk → /dev/rdisk and
            // checks if drive.path matches the normalized version
            let drive = make_drive("/dev/rdisk2");
            let drives = vec![drive];

            // Searching for /dev/disk2 should normalize to /dev/rdisk2 and find it
            let result = find_drive(&drives, "/dev/disk2");
            assert!(result.is_ok());
        }
    }

    #[test]
    fn test_find_drive_not_found_empty_list() {
        let drives: Vec<Drive> = vec![];
        let result = find_drive(&drives, "/dev/sdb");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("not found"), "Error: {}", err_msg);
        assert!(
            err_msg.contains("engraver list"),
            "Should suggest 'engraver list': {}",
            err_msg
        );
    }

    #[test]
    fn test_find_drive_not_found_among_other_drives() {
        let drives = vec![make_drive("/dev/sda"), make_drive("/dev/sdc")];

        let result = find_drive(&drives, "/dev/sdb");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("/dev/sdb"));
    }

    #[test]
    fn test_find_drive_partition_error() {
        let mut drive = make_drive("/dev/sdb");
        drive.partitions = vec![engraver_detect::Partition {
            path: "/dev/sdb1".to_string(),
            label: Some("DATA".to_string()),
            filesystem: Some("ext4".to_string()),
            size: 8 * 1024 * 1024 * 1024,
            mount_point: Some("/media/data".to_string()),
        }];
        let drives = vec![drive];

        // Trying to erase a partition should suggest the whole device
        let result = find_drive(&drives, "/dev/sdb1");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("partition"),
            "Should mention partition: {}",
            err_msg
        );
        assert!(
            err_msg.contains("/dev/sdb"),
            "Should suggest whole device: {}",
            err_msg
        );
    }

    #[test]
    fn test_find_drive_partition_among_multiple_drives() {
        let mut drive1 = make_drive("/dev/sda");
        drive1.partitions = vec![engraver_detect::Partition {
            path: "/dev/sda1".to_string(),
            label: None,
            filesystem: None,
            size: 0,
            mount_point: None,
        }];

        let mut drive2 = make_drive("/dev/sdb");
        drive2.partitions = vec![
            engraver_detect::Partition {
                path: "/dev/sdb1".to_string(),
                label: None,
                filesystem: None,
                size: 0,
                mount_point: None,
            },
            engraver_detect::Partition {
                path: "/dev/sdb2".to_string(),
                label: None,
                filesystem: None,
                size: 0,
                mount_point: None,
            },
        ];

        let drives = vec![drive1, drive2];

        // Should find partition on the correct drive
        let result = find_drive(&drives, "/dev/sdb2");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("/dev/sdb"), "Error: {}", err_msg);
    }

    #[test]
    fn test_find_drive_selects_first_match() {
        // If multiple drives match (unlikely but test the code path), first wins
        let drives = vec![make_drive("/dev/sdb"), make_drive("/dev/sdb")];
        let result = find_drive(&drives, "/dev/sdb");
        assert!(result.is_ok());
    }

    // =========================================================================
    // create_erase_progress_bar tests
    // =========================================================================

    #[test]
    fn test_create_erase_progress_bar_silent() {
        let pb = create_erase_progress_bar(1024, true);
        assert!(pb.is_hidden());
    }

    #[test]
    fn test_create_erase_progress_bar_visible() {
        let pb = create_erase_progress_bar(1024 * 1024, false);
        assert_eq!(pb.length(), Some(1024 * 1024));
        // ProgressBar created with ProgressBar::new() has a style set and correct length
        // (is_hidden may be true in non-TTY test environments, so we just check length)
    }

    #[test]
    fn test_create_erase_progress_bar_zero_size() {
        let pb = create_erase_progress_bar(0, false);
        assert_eq!(pb.length(), Some(0));
    }

    #[test]
    fn test_create_erase_progress_bar_large_size() {
        let size = 1024u64 * 1024 * 1024 * 1024; // 1 TB
        let pb = create_erase_progress_bar(size, false);
        assert_eq!(pb.length(), Some(size));
    }

    // =========================================================================
    // EraseArgs tests
    // =========================================================================

    #[test]
    fn test_erase_args_default_flags() {
        let args = EraseArgs {
            target: "/dev/sdb".to_string(),
            skip_confirm: false,
            block_size: "4M".to_string(),
            force: false,
            no_unmount: false,
            cancel_flag: Arc::new(AtomicBool::new(true)),
            silent: false,
        };

        assert_eq!(args.target, "/dev/sdb");
        assert!(!args.skip_confirm);
        assert_eq!(args.block_size, "4M");
        assert!(!args.force);
        assert!(!args.no_unmount);
        assert!(args.cancel_flag.load(Ordering::Relaxed));
        assert!(!args.silent);
    }

    #[test]
    fn test_erase_args_all_flags_enabled() {
        let args = EraseArgs {
            target: "/dev/disk2".to_string(),
            skip_confirm: true,
            block_size: "1M".to_string(),
            force: true,
            no_unmount: true,
            cancel_flag: Arc::new(AtomicBool::new(false)),
            silent: true,
        };

        assert_eq!(args.target, "/dev/disk2");
        assert!(args.skip_confirm);
        assert_eq!(args.block_size, "1M");
        assert!(args.force);
        assert!(args.no_unmount);
        assert!(!args.cancel_flag.load(Ordering::Relaxed));
        assert!(args.silent);
    }

    #[test]
    fn test_erase_args_cancel_flag_shared() {
        let cancel = Arc::new(AtomicBool::new(false));
        let cancel_clone = cancel.clone();

        let args = EraseArgs {
            target: "/dev/sdb".to_string(),
            skip_confirm: true,
            block_size: "4M".to_string(),
            force: false,
            no_unmount: false,
            cancel_flag: cancel,
            silent: false,
        };

        // Simulate cancellation from another thread
        cancel_clone.store(true, Ordering::SeqCst);
        assert!(args.cancel_flag.load(Ordering::SeqCst));
    }
}
