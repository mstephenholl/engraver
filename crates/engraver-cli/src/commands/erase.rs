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
            style("║  The entire device will be zero-filled.                     ║")
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
        style("✓ Erase complete! The device has been zero-filled.")
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
        assert!(parse_block_size("abc").is_err());
    }

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(1024 * 1024), "1.00 MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.00 GB");
    }

    #[test]
    fn test_format_eta() {
        assert_eq!(format_eta(0.0), "0s");
        assert_eq!(format_eta(30.0), "30s");
        assert_eq!(format_eta(90.0), "1m 30s");
        assert_eq!(format_eta(3661.0), "1h 1m 1s");
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

    #[test]
    fn test_find_drive_by_path() {
        let drives = vec![engraver_detect::Drive {
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
        let drives = vec![];
        let result = find_drive(&drives, "/dev/sdb");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_create_erase_progress_bar_silent() {
        let pb = create_erase_progress_bar(1024, true);
        assert!(pb.is_hidden());
    }

    #[test]
    fn test_create_erase_progress_bar_with_size() {
        let pb = create_erase_progress_bar(1024 * 1024, false);
        assert_eq!(pb.length(), Some(1024 * 1024));
    }

    #[test]
    fn test_erase_args_creation() {
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
}
