//! Engraver - A safe, fast tool for creating bootable USB drives
//!
//! # Usage
//!
//! ```bash
//! # List available drives
//! engraver list
//!
//! # Write an ISO to a USB drive
//! engraver write ubuntu.iso /dev/sdb
//!
//! # Write with verification
//! engraver write ubuntu.iso /dev/sdb --verify
//!
//! # Write from URL
//! engraver write https://releases.ubuntu.com/24.04/ubuntu.iso /dev/sdb
//! ```

use anyhow::Result;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use clap_mangen::Man;
use console::style;
use tracing_subscriber::EnvFilter;

mod commands;
mod progress;

/// Engraver - A safe, fast tool for creating bootable USB drives
#[derive(Parser)]
#[command(name = "engraver")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Suppress all output except errors
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Suppress ALL output (implies --quiet and --yes)
    #[arg(long, global = true)]
    silent: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List available drives
    List {
        /// Show all drives including system drives
        #[arg(short, long)]
        all: bool,

        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },

    /// Write an image to a drive
    Write {
        /// Source image (local file or URL)
        source: String,

        /// Target device (e.g., /dev/sdb, /dev/disk2, \\.\PhysicalDrive1)
        target: String,

        /// Verify write by reading back and comparing
        #[arg(long)]
        verify: bool,

        /// Skip confirmation prompt (use with caution!)
        #[arg(short = 'y', long)]
        yes: bool,

        /// Block size for writing (e.g., 4M, 1M, 512K)
        #[arg(short, long, default_value = "4M")]
        block_size: String,

        /// Verify checksum against expected value
        #[arg(long, value_name = "CHECKSUM")]
        checksum: Option<String>,

        /// Checksum algorithm (sha256, sha512, md5)
        #[arg(long, default_value = "sha256")]
        checksum_algo: String,

        /// Force write even to system drives (DANGEROUS!)
        #[arg(long, hide = true)]
        force: bool,

        /// Do not unmount partitions before writing
        #[arg(long)]
        no_unmount: bool,

        /// Resume an interrupted write operation
        #[arg(long)]
        resume: bool,

        /// Enable checkpointing for resume support (auto-enabled with --resume)
        #[arg(long)]
        checkpoint: bool,
    },

    /// Verify a drive against a source image
    Verify {
        /// Source image (local file or URL)
        source: String,

        /// Target device to verify
        target: String,

        /// Block size for reading
        #[arg(short, long, default_value = "4M")]
        block_size: String,
    },

    /// Calculate checksum of an image
    Checksum {
        /// Source image (local file or URL)
        source: String,

        /// Checksum algorithm (sha256, sha512, md5, crc32)
        #[arg(short, long, default_value = "sha256")]
        algorithm: String,
    },

    /// Generate shell completions
    Completions {
        /// Shell to generate completions for
        #[arg(value_enum)]
        shell: Shell,
    },

    /// Generate man pages
    Mangen {
        /// Output directory for man pages
        #[arg(short, long, default_value = ".")]
        out_dir: String,
    },
}

fn main() {
    // Set up panic handler for nicer error messages
    std::panic::set_hook(Box::new(|panic_info| {
        eprintln!("{} {}", style("Error:").red().bold(), panic_info);
    }));

    if let Err(e) = run() {
        eprintln!("{} {}", style("Error:").red().bold(), e);

        // Show cause chain in verbose mode
        if std::env::var("RUST_BACKTRACE").is_ok() {
            let mut source = e.source();
            while let Some(cause) = source {
                eprintln!("  {} {}", style("Caused by:").yellow(), cause);
                source = cause.source();
            }
        }

        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    // --silent implies --quiet (no logs at all, not even errors to tracing)
    let filter = if cli.verbose {
        EnvFilter::new("debug")
    } else if cli.quiet || cli.silent {
        EnvFilter::new("off")
    } else {
        EnvFilter::new("info")
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .without_time()
        .init();

    // --silent implies --yes (skip confirmations)
    let silent = cli.silent;

    // Set up Ctrl+C handler (suppress messages in silent mode)
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();
    let silent_for_handler = silent;
    ctrlc::set_handler(move || {
        if !r.load(std::sync::atomic::Ordering::SeqCst) {
            // Second Ctrl+C, force exit
            if !silent_for_handler {
                eprintln!("\n{}", style("Forced exit").red().bold());
            }
            std::process::exit(130);
        }
        r.store(false, std::sync::atomic::Ordering::SeqCst);
        if !silent_for_handler {
            eprintln!(
                "\n{}",
                style("Cancelling... Press Ctrl+C again to force exit").yellow()
            );
        }
    })?;

    match cli.command {
        Commands::List { all, json } => commands::list::execute(all, json, silent),
        Commands::Write {
            source,
            target,
            verify,
            yes,
            block_size,
            checksum,
            checksum_algo,
            force,
            no_unmount,
            resume,
            checkpoint,
        } => {
            commands::write::execute(commands::write::WriteArgs {
                source,
                target,
                verify,
                skip_confirm: yes || silent, // --silent implies --yes
                block_size,
                checksum,
                checksum_algo,
                force,
                no_unmount,
                cancel_flag: running,
                silent,
                resume,
                checkpoint: checkpoint || resume, // --resume implies --checkpoint
            })
        }
        Commands::Verify {
            source,
            target,
            block_size,
        } => commands::verify::execute(&source, &target, &block_size, running, silent),
        Commands::Checksum { source, algorithm } => {
            commands::checksum::execute(&source, &algorithm, silent)
        }
        Commands::Completions { shell } => {
            let mut cmd = Cli::command();
            let name = cmd.get_name().to_string();
            generate(shell, &mut cmd, name, &mut std::io::stdout());
            Ok(())
        }
        Commands::Mangen { out_dir } => {
            let cmd = Cli::command();
            let out_path = std::path::Path::new(&out_dir);
            std::fs::create_dir_all(out_path)?;

            // Generate main man page
            let man = Man::new(cmd.clone());
            let mut buffer = Vec::new();
            man.render(&mut buffer)?;
            std::fs::write(out_path.join("engraver.1"), buffer)?;
            if !silent {
                println!("Generated: {}/engraver.1", out_dir);
            }

            // Generate man pages for subcommands
            for subcommand in cmd.get_subcommands() {
                let name = subcommand.get_name();
                // Skip hidden commands and meta commands
                if subcommand.is_hide_set()
                    || name == "completions"
                    || name == "mangen"
                    || name == "help"
                {
                    continue;
                }

                let man = Man::new(subcommand.clone());
                let mut buffer = Vec::new();
                man.render(&mut buffer)?;
                let filename = format!("engraver-{}.1", name);
                std::fs::write(out_path.join(&filename), buffer)?;
                if !silent {
                    println!("Generated: {}/{}", out_dir, filename);
                }
            }

            if !silent {
                println!(
                    "\nInstall with: sudo cp {}/*.1 /usr/local/share/man/man1/",
                    out_dir
                );
            }
            Ok(())
        }
    }
}
