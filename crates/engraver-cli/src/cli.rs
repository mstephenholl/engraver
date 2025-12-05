//! CLI argument parsing and command structure

use anyhow::Result;
use clap::{Parser, Subcommand};

/// Engraver - Create bootable USB drives from ISO images
///
/// A fast, safe CLI tool for creating bootable USB drives, SD cards,
/// and NVMe drives from ISO images.
#[derive(Parser)]
#[command(name = "engraver")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
pub struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    pub verbose: bool,

    /// Output format (text, json)
    #[arg(short, long, default_value = "text", global = true)]
    pub format: String,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Write an image to a target device
    Write {
        /// Path to the source image (ISO, IMG) or URL
        #[arg(short, long)]
        source: String,

        /// Target device path (e.g., /dev/sdb)
        #[arg(short, long)]
        target: String,

        /// Skip verification after writing
        #[arg(long)]
        no_verify: bool,

        /// Skip confirmation prompt (use with caution!)
        #[arg(short = 'y', long)]
        yes: bool,

        /// Block size for write operations (e.g., 4M, 1M)
        #[arg(long, default_value = "4M")]
        block_size: String,
    },

    /// List available removable drives
    List {
        /// Include all drives (not just removable)
        #[arg(long)]
        all: bool,
    },

    /// Verify a device against a source image
    Verify {
        /// Path to the source image
        #[arg(short, long)]
        source: String,

        /// Target device to verify
        #[arg(short, long)]
        target: String,
    },

    /// Calculate checksum of a file or device
    Checksum {
        /// Path to file or device
        path: String,

        /// Algorithm (sha256, sha512, md5)
        #[arg(short, long, default_value = "sha256")]
        algorithm: String,
    },
}

impl Cli {
    pub fn execute(self) -> Result<()> {
        match self.command {
            Commands::Write {
                source,
                target,
                no_verify,
                yes,
                block_size,
            } => crate::commands::write::execute(source, target, !no_verify, yes, block_size),
            Commands::List { all } => crate::commands::list::execute(all),
            Commands::Verify { source, target } => crate::commands::verify::execute(source, target),
            Commands::Checksum { path, algorithm } => {
                crate::commands::checksum::execute(path, algorithm)
            }
        }
    }
}
