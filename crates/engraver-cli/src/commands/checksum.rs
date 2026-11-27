//! Checksum command - calculates checksum of an image

use anyhow::{Context, Result};
use console::style;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use engraver_core::{
    validate_source_with_options, ChecksumAlgorithm, Source, SourceOptions, Verifier, VerifyConfig,
};

use super::{context_or_cancelled, create_progress_bar, describe_source_size};

/// Execute the checksum command
pub fn execute(
    source: &str,
    algorithm: &str,
    options: &SourceOptions,
    cancel_flag: Arc<AtomicBool>,
    silent: bool,
) -> Result<()> {
    // Parse algorithm
    let algo: ChecksumAlgorithm = algorithm
        .parse()
        .with_context(|| format!("Invalid algorithm: {}", algorithm))?;

    // Validate source
    println_if!(
        silent,
        "{} {}",
        style("Source:").bold(),
        style(source).cyan()
    );

    let source_info = validate_source_with_options(source, options)
        .with_context(|| format!("Failed to validate source: {}", source))?;

    if let Some(size) = describe_source_size(&source_info, format_size) {
        println_if!(silent, "  Size: {}", size);
    }

    // Open source
    println_if!(
        silent,
        "\n{} {} checksum...",
        style("Calculating").bold(),
        algo.name()
    );

    let mut source_reader = Source::open_with_options(source, 0, options)
        .with_context(|| format!("Failed to open source: {}", source))?;

    let pb = create_progress_bar(source_info.size, "Checksumming", silent);

    // Calculate checksum
    let config = VerifyConfig::new();
    let pb_clone = pb.clone();
    let mut verifier = Verifier::with_config(config)
        .with_cancel_flag(cancel_flag)
        .on_progress(move |progress| {
            pb_clone.set_position(progress.bytes_processed);
        });

    let checksum = verifier
        .calculate_checksum(&mut source_reader, algo, source_info.size)
        .map_err(|e| {
            context_or_cancelled(e, "Failed to calculate checksum", "Checksum cancelled.")
        })?;

    pb.finish_and_clear();

    // Output result - always print the checksum hash even in silent mode (it's the useful output)
    if silent {
        // In silent mode, just output the bare checksum
        println!("{}", checksum.to_hex());
    } else {
        println!();
        println!("{} ({}):", style(algo.name()).green().bold(), source);
        println!("{}", checksum.to_hex());

        // Also output in common checksum file format
        println!();
        println!("{}:", style("Checksum file format").dim());
        println!(
            "{}  {}",
            checksum.to_hex(),
            source.split('/').next_back().unwrap_or(source)
        );
    }

    Ok(())
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
    use std::io::Write;

    // -------------------------------------------------------------------------
    // execute cancellation tests
    // -------------------------------------------------------------------------

    fn source_file(len: usize) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(&vec![0xABu8; len]).unwrap();
        file
    }

    #[test]
    fn test_execute_with_pending_cancel_returns_a_cancelled_error() {
        let file = source_file(64 * 1024);
        let cancel = Arc::new(AtomicBool::new(true));

        let err = execute(
            file.path().to_str().unwrap(),
            "sha256",
            &SourceOptions::default(),
            cancel,
            true,
        )
        .expect_err("a pending cancel must stop the checksum");

        crate::commands::assert_cancelled(&err);
    }

    #[test]
    fn test_execute_without_cancel_succeeds() {
        let file = source_file(64 * 1024);
        let cancel = Arc::new(AtomicBool::new(false));

        execute(
            file.path().to_str().unwrap(),
            "sha256",
            &SourceOptions::default(),
            cancel,
            true,
        )
        .expect("an uncancelled checksum must succeed");
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
        assert_eq!(format_size(10 * 1024), "10.00 KB");
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

    #[test]
    fn test_format_size_common_iso_sizes() {
        // Common ISO image sizes
        assert_eq!(format_size(700 * 1024 * 1024), "700.00 MB"); // CD image
        assert_eq!(format_size(4700u64 * 1024 * 1024), "4.59 GB"); // DVD image
        assert_eq!(format_size(3u64 * 1024 * 1024 * 1024), "3.00 GB"); // Ubuntu ISO
    }
}
