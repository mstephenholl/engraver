//! Verify command - verifies a drive against a source image

use anyhow::{bail, Context, Result};
use console::style;
use indicatif::{ProgressBar, ProgressStyle};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use engraver_core::{
    validate_source, ChecksumAlgorithm, Source, SourceType, Verifier, VerifyConfig,
};
use engraver_detect::list_drives;
use engraver_platform::{has_elevated_privileges, open_device, OpenOptions};

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

/// Execute the verify command
pub fn execute(
    source: &str,
    target: &str,
    block_size_str: &str,
    cancel_flag: Arc<AtomicBool>,
    silent: bool,
) -> Result<()> {
    // Parse block size
    let block_size = parse_block_size(block_size_str)?;

    // Check for elevated privileges (needed for raw device access)
    if !has_elevated_privileges() {
        #[cfg(unix)]
        bail!(
            "Root privileges required.\n\
             Try running with: sudo engraver verify ..."
        );

        #[cfg(windows)]
        bail!(
            "Administrator privileges required.\n\
             Right-click and select 'Run as administrator'."
        );

        #[cfg(not(any(unix, windows)))]
        bail!("Elevated privileges required for raw device access.");
    }

    // Validate source
    println_if!(
        silent,
        "{} {}",
        style("Source:").bold(),
        style(source).cyan()
    );

    let source_info = validate_source(source)
        .with_context(|| format!("Failed to validate source: {}", source))?;

    let source_size = source_info.size.or(source_info.compressed_size);
    let source_is_local = source_info.source_type == SourceType::LocalFile;

    if let Some(size) = source_size {
        println_if!(silent, "  {} ({})", style("✓").green(), format_size(size));
    } else {
        println_if!(silent, "  {} (size unknown)", style("✓").green());
    }

    // Validate target
    println_if!(
        silent,
        "\n{} {}",
        style("Target:").bold(),
        style(target).cyan()
    );

    let drives = list_drives().context("Failed to list drives")?;
    let target_drive = drives
        .iter()
        .find(|d| d.path == target || d.raw_path == target);

    if let Some(drive) = target_drive {
        println_if!(
            silent,
            "  {} {} ({})",
            style("✓").green(),
            drive.display_name(),
            format_size(drive.size)
        );
    } else {
        println_if!(silent, "  {} Device found", style("✓").green());
    }

    // Open target device for reading using platform layer
    let device_path = get_raw_device_path(target);
    let options = OpenOptions::new()
        .read(true)
        .write(false)
        .direct_io(false) // Don't need direct I/O for reading
        .block_size(block_size);

    let mut target_reader = open_device(&device_path, options)
        .with_context(|| format!("Failed to open device: {}", device_path))?;

    let total_size = source_size.unwrap_or(0);

    println_if!(silent, "\n{}", style("Verifying...").bold());

    // Set up cancel handler
    let cancel_clone = cancel_flag.clone();

    if source_is_local {
        // Direct byte-by-byte comparison for local files
        let mut source_file = std::fs::File::open(source)
            .with_context(|| format!("Failed to open source: {}", source))?;

        // Create progress bar
        let pb = if silent {
            ProgressBar::hidden()
        } else if total_size > 0 {
            ProgressBar::new(total_size)
        } else {
            ProgressBar::new_spinner()
        };

        if !silent {
            pb.set_style(
                ProgressStyle::default_bar()
                    .template(
                        "  {spinner:.green} Comparing [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})",
                    )
                    .unwrap()
                    .progress_chars("█▓░"),
            );
        }

        // Set up verifier
        let config = VerifyConfig::new().block_size(block_size);
        let verifier = Verifier::with_config(config);

        // Connect cancel flag
        let verifier_cancel = verifier.cancel_handle();
        std::thread::spawn(move || {
            while cancel_clone.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            verifier_cancel.store(true, Ordering::SeqCst);
        });

        // Add progress callback
        let pb_clone = pb.clone();
        let verifier = verifier.on_progress(move |progress| {
            pb_clone.set_position(progress.bytes_processed);
        });

        let mut verifier = verifier;
        let result = verifier.compare(&mut source_file, &mut *target_reader, total_size);

        pb.finish_and_clear();

        handle_verify_result(result, silent)
    } else {
        // For remote/compressed sources, compare checksums
        println_if!(
            silent,
            "  {} Source is remote/compressed, using checksum verification",
            style("ℹ").blue()
        );

        // Calculate checksum of target
        let pb = if silent {
            ProgressBar::hidden()
        } else {
            ProgressBar::new(total_size)
        };
        if !silent {
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("  {spinner:.green} Checksumming target [{bar:40.cyan/blue}] {bytes}/{total_bytes}")
                    .unwrap()
                    .progress_chars("█▓░"),
            );
        }

        let config = VerifyConfig::new().block_size(block_size);
        let pb_clone = pb.clone();
        let mut verifier = Verifier::with_config(config).on_progress(move |p| {
            pb_clone.set_position(p.bytes_processed);
        });

        let target_checksum = verifier
            .calculate_checksum(
                &mut *target_reader,
                ChecksumAlgorithm::Sha256,
                Some(total_size),
            )
            .context("Failed to checksum target")?;

        pb.finish_and_clear();

        // Calculate checksum of source
        println_if!(silent, "  Calculating source checksum...");
        let mut source_reader =
            Source::open(source).with_context(|| format!("Failed to open source: {}", source))?;

        let pb = if silent {
            ProgressBar::hidden()
        } else if let Some(size) = source_size {
            ProgressBar::new(size)
        } else {
            ProgressBar::new_spinner()
        };
        if !silent {
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("  {spinner:.green} Checksumming source [{bar:40.cyan/blue}] {bytes}/{total_bytes}")
                    .unwrap()
                    .progress_chars("█▓░"),
            );
        }

        let config = VerifyConfig::new().block_size(block_size);
        let pb_clone = pb.clone();
        let mut verifier = Verifier::with_config(config).on_progress(move |p| {
            pb_clone.set_position(p.bytes_processed);
        });

        let source_checksum = verifier
            .calculate_checksum(&mut source_reader, ChecksumAlgorithm::Sha256, source_size)
            .context("Failed to checksum source")?;

        pb.finish_and_clear();

        if target_checksum.matches(&source_checksum) {
            println_if!(
                silent,
                "  {} Checksum verification passed!",
                style("✓").green().bold()
            );
            println_if!(silent, "    SHA-256: {}", source_checksum.to_hex());
            Ok(())
        } else {
            println_if!(
                silent,
                "  {} Checksum verification FAILED!",
                style("✗").red().bold()
            );
            println_if!(silent, "    Source:  {}", source_checksum.to_hex());
            println_if!(silent, "    Target:  {}", target_checksum.to_hex());
            bail!("Verification failed: checksums do not match");
        }
    }
}

/// Handle verification result
fn handle_verify_result(
    result: std::result::Result<engraver_core::VerificationResult, engraver_core::Error>,
    silent: bool,
) -> Result<()> {
    match result {
        Ok(result) if result.success => {
            println_if!(
                silent,
                "  {} Verification passed!",
                style("✓").green().bold()
            );
            println_if!(
                silent,
                "    {} bytes verified in {:.1}s ({}/s)",
                result.bytes_verified,
                result.elapsed.as_secs_f64(),
                format_size(result.speed_bps)
            );
            Ok(())
        }
        Ok(result) => {
            println_if!(silent, "  {} Verification FAILED!", style("✗").red().bold());
            println_if!(silent, "    {} mismatch(es) found", result.mismatches);
            if let Some(offset) = result.first_mismatch_offset {
                println_if!(silent, "    First mismatch at byte offset: {}", offset);
            }
            bail!("Verification failed");
        }
        Err(engraver_core::Error::Cancelled) => {
            println_if!(silent, "\n{}", style("Verification cancelled.").yellow());
            Ok(())
        }
        Err(e) => {
            bail!("Verification error: {}", e);
        }
    }
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

/// Parse a human-readable block size
fn parse_block_size(s: &str) -> Result<usize> {
    let s = s.trim().to_uppercase();

    let (num_str, multiplier) = if s.ends_with('K') {
        (&s[..s.len() - 1], 1024)
    } else if s.ends_with('M') {
        (&s[..s.len() - 1], 1024 * 1024)
    } else {
        (s.as_str(), 1)
    };

    let num: usize = num_str
        .parse()
        .with_context(|| format!("Invalid block size: {}", s))?;

    Ok(num * multiplier)
}

/// Format size for display
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // parse_block_size tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_parse_block_size_bytes() {
        assert_eq!(parse_block_size("4096").unwrap(), 4096);
        assert_eq!(parse_block_size("8192").unwrap(), 8192);
        assert_eq!(parse_block_size("1048576").unwrap(), 1048576);
    }

    #[test]
    fn test_parse_block_size_kilobytes() {
        assert_eq!(parse_block_size("4K").unwrap(), 4096);
        assert_eq!(parse_block_size("4k").unwrap(), 4096);
        assert_eq!(parse_block_size("8K").unwrap(), 8192);
        assert_eq!(parse_block_size("512K").unwrap(), 512 * 1024);
    }

    #[test]
    fn test_parse_block_size_megabytes() {
        assert_eq!(parse_block_size("1M").unwrap(), 1024 * 1024);
        assert_eq!(parse_block_size("4M").unwrap(), 4 * 1024 * 1024);
        assert_eq!(parse_block_size("16M").unwrap(), 16 * 1024 * 1024);
    }

    #[test]
    fn test_parse_block_size_with_whitespace() {
        assert_eq!(parse_block_size("  4K  ").unwrap(), 4096);
        assert_eq!(parse_block_size("\t1M\n").unwrap(), 1024 * 1024);
    }

    #[test]
    fn test_parse_block_size_invalid() {
        assert!(parse_block_size("abc").is_err());
        assert!(parse_block_size("").is_err());
        assert!(parse_block_size("K").is_err());
        assert!(parse_block_size("-1K").is_err());
    }

    // -------------------------------------------------------------------------
    // format_size tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(1), "1 B");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1023), "1023 B");
    }

    #[test]
    fn test_format_size_kilobytes() {
        assert_eq!(format_size(1024), "1.00 KB");
        assert_eq!(format_size(2048), "2.00 KB");
        assert_eq!(format_size(1536), "1.50 KB");
    }

    #[test]
    fn test_format_size_megabytes() {
        assert_eq!(format_size(1024 * 1024), "1.00 MB");
        assert_eq!(format_size(100 * 1024 * 1024), "100.00 MB");
        assert_eq!(format_size(1024 * 1024 + 512 * 1024), "1.50 MB");
    }

    #[test]
    fn test_format_size_gigabytes() {
        assert_eq!(format_size(1024 * 1024 * 1024), "1.00 GB");
        assert_eq!(format_size(4u64 * 1024 * 1024 * 1024), "4.00 GB");
        assert_eq!(format_size(32u64 * 1024 * 1024 * 1024), "32.00 GB");
    }

    // -------------------------------------------------------------------------
    // get_raw_device_path tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_get_raw_device_path_linux() {
        #[cfg(target_os = "linux")]
        {
            assert_eq!(get_raw_device_path("/dev/sda"), "/dev/sda");
            assert_eq!(get_raw_device_path("/dev/sdb"), "/dev/sdb");
            assert_eq!(get_raw_device_path("/dev/nvme0n1"), "/dev/nvme0n1");
            assert_eq!(get_raw_device_path("/dev/mmcblk0"), "/dev/mmcblk0");
        }
    }

    #[test]
    fn test_get_raw_device_path_macos() {
        #[cfg(target_os = "macos")]
        {
            assert_eq!(get_raw_device_path("/dev/disk2"), "/dev/rdisk2");
            assert_eq!(get_raw_device_path("/dev/rdisk2"), "/dev/rdisk2");
            assert_eq!(get_raw_device_path("/dev/disk0"), "/dev/rdisk0");
        }
    }

    #[test]
    fn test_get_raw_device_path_passthrough() {
        // Non-standard paths should pass through unchanged
        assert_eq!(get_raw_device_path("/tmp/image.img"), "/tmp/image.img");
        assert_eq!(get_raw_device_path("./local/file"), "./local/file");
    }

    // -------------------------------------------------------------------------
    // handle_verify_result tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_handle_verify_result_success() {
        let result = engraver_core::VerificationResult {
            success: true,
            bytes_verified: 1024 * 1024,
            mismatches: 0,
            first_mismatch_offset: None,
            elapsed: std::time::Duration::from_secs(1),
            speed_bps: 1024 * 1024,
        };

        let handled = handle_verify_result(Ok(result), true);
        assert!(handled.is_ok());
    }

    #[test]
    fn test_handle_verify_result_failure() {
        let result = engraver_core::VerificationResult {
            success: false,
            bytes_verified: 512 * 1024,
            mismatches: 5,
            first_mismatch_offset: Some(1024),
            elapsed: std::time::Duration::from_secs(1),
            speed_bps: 512 * 1024,
        };

        let handled = handle_verify_result(Ok(result), true);
        assert!(handled.is_err());
        let err = handled.unwrap_err().to_string();
        assert!(err.contains("failed"));
    }

    #[test]
    fn test_handle_verify_result_cancelled() {
        let handled = handle_verify_result(Err(engraver_core::Error::Cancelled), true);
        // Cancelled is not an error - returns Ok
        assert!(handled.is_ok());
    }

    #[test]
    fn test_handle_verify_result_error() {
        let io_err = std::io::Error::other("disk error");
        let handled = handle_verify_result(Err(engraver_core::Error::Io(io_err)), true);
        assert!(handled.is_err());
    }
}
