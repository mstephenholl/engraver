//! Checksum command - calculates checksum of an image

use anyhow::{Context, Result};
use console::style;
use indicatif::{ProgressBar, ProgressStyle};

use engraver_core::{validate_source, ChecksumAlgorithm, Source, Verifier, VerifyConfig};

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

/// Execute the checksum command
pub fn execute(source: &str, algorithm: &str, silent: bool) -> Result<()> {
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

    let source_info = validate_source(source)
        .with_context(|| format!("Failed to validate source: {}", source))?;

    let source_size = source_info.size.or(source_info.compressed_size);

    if let Some(size) = source_size {
        println_if!(silent, "  Size: {}", format_size(size));
    }

    // Open source
    println_if!(
        silent,
        "\n{} {} checksum...",
        style("Calculating").bold(),
        algo.name()
    );

    let mut source_reader =
        Source::open(source).with_context(|| format!("Failed to open source: {}", source))?;

    // Create progress bar
    let pb = if silent {
        ProgressBar::hidden()
    } else {
        match source_size {
            Some(size) => ProgressBar::new(size),
            None => ProgressBar::new_spinner(),
        }
    };

    if !silent {
        pb.set_style(
            ProgressStyle::default_bar()
                .template("  {spinner:.green} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})")
                .unwrap()
                .progress_chars("█▓░"),
        );
    }

    // Calculate checksum
    let config = VerifyConfig::new();
    let pb_clone = pb.clone();
    let mut verifier = Verifier::with_config(config).on_progress(move |progress| {
        pb_clone.set_position(progress.bytes_processed);
    });

    let checksum = verifier
        .calculate_checksum(&mut source_reader, algo, source_size)
        .context("Failed to calculate checksum")?;

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
