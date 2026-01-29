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
use engraver_core::Settings;
use std::path::PathBuf;
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

    /// Use a custom configuration file instead of the default
    #[arg(long, global = true, value_name = "PATH")]
    config_file: Option<PathBuf>,

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

        /// Verify write by reading back and comparing (can be set in config)
        #[arg(long)]
        verify: bool,

        /// Skip confirmation prompt (use with caution!)
        #[arg(short = 'y', long)]
        yes: bool,

        /// Block size for writing (e.g., 4M, 1M, 512K). Default from config or 4M
        #[arg(short, long)]
        block_size: Option<String>,

        /// Verify checksum against expected value
        #[arg(long, value_name = "CHECKSUM")]
        checksum: Option<String>,

        /// Checksum algorithm (sha256, sha512, md5). Default from config or sha256
        #[arg(long)]
        checksum_algo: Option<String>,

        /// Force write even to system drives (DANGEROUS!)
        #[arg(long, hide = true)]
        force: bool,

        /// Do not unmount partitions before writing
        #[arg(long)]
        no_unmount: bool,

        /// Resume an interrupted write operation
        #[arg(long)]
        resume: bool,

        /// Enable checkpointing for resume support (auto-enabled with --resume, can be set in config)
        #[arg(long)]
        checkpoint: bool,

        /// Automatically detect and verify checksum from .sha256, .sha512, .md5 files
        #[arg(long)]
        auto_checksum: bool,

        /// Show partition layout of source image before writing
        #[arg(long, short = 'p')]
        show_partitions: bool,
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

        /// Checksum algorithm (sha256, sha512, md5, crc32). Default from config or sha256
        #[arg(short, long)]
        algorithm: Option<String>,
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

    /// Manage configuration settings
    Config {
        /// Initialize a new configuration file with defaults
        #[arg(long)]
        init: bool,

        /// Show the path to the configuration file
        #[arg(long)]
        path: bool,

        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },

    /// Benchmark write speed of a drive (DESTRUCTIVE)
    Benchmark {
        /// Target device (e.g., /dev/sdb, \\.\PhysicalDrive1)
        target: String,

        /// Amount of data to write for the test (e.g., 256M, 1G). Mutually exclusive with --test-block-sizes. Default from config or 256M
        #[arg(short, long)]
        size: Option<String>,

        /// Block size for writing (e.g., 4K, 1M, 4M). Must be power of 2, max 64M. Default from config or 4M
        #[arg(short, long)]
        block_size: Option<String>,

        /// Data pattern: zeros, random, sequential. Default from config or zeros
        #[arg(long)]
        pattern: Option<String>,

        /// Number of benchmark passes. Default from config or 1
        #[arg(long)]
        passes: Option<u32>,

        /// Test multiple block sizes (comma-separated, e.g., "4K,64K,1M,4M,16M"). Mutually exclusive with --size
        #[arg(long)]
        test_block_sizes: Option<String>,

        /// Output in JSON format
        #[arg(long)]
        json: bool,

        /// Skip confirmation prompt (DANGEROUS!)
        #[arg(short = 'y', long)]
        yes: bool,
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

    // Load user settings from config file (custom path takes precedence)
    let settings = if let Some(ref config_path) = cli.config_file {
        Settings::load_from_path(Some(config_path.clone()))
    } else {
        Settings::load()
    };

    // Initialize logging
    // --silent implies --quiet (no logs at all, not even errors to tracing)
    // Settings can also set quiet mode
    let filter = if cli.verbose {
        EnvFilter::new("debug")
    } else if cli.quiet || cli.silent || settings.behavior.quiet {
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
            auto_checksum,
            show_partitions,
        } => {
            // Apply settings as defaults when CLI options are not explicitly set
            let effective_block_size =
                block_size.unwrap_or_else(|| settings.write.block_size.clone());
            let effective_checksum_algo =
                checksum_algo.unwrap_or_else(|| settings.checksum.algorithm.clone());
            // CLI flags || settings defaults
            let effective_verify = verify || settings.write.verify;
            let effective_checkpoint = checkpoint || resume || settings.write.checkpoint;
            let effective_skip_confirm = yes || silent || settings.behavior.skip_confirmation;
            let effective_auto_checksum = auto_checksum || settings.checksum.auto_detect;

            commands::write::execute(commands::write::WriteArgs {
                source,
                target,
                verify: effective_verify,
                skip_confirm: effective_skip_confirm,
                block_size: effective_block_size,
                checksum,
                checksum_algo: effective_checksum_algo,
                force,
                no_unmount,
                cancel_flag: running,
                silent,
                resume,
                checkpoint: effective_checkpoint,
                auto_checksum: effective_auto_checksum,
                show_partitions,
            })
        }
        Commands::Verify {
            source,
            target,
            block_size,
        } => commands::verify::execute(&source, &target, &block_size, running, silent),
        Commands::Checksum { source, algorithm } => {
            let effective_algorithm =
                algorithm.unwrap_or_else(|| settings.checksum.algorithm.clone());
            commands::checksum::execute(&source, &effective_algorithm, silent)
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
        Commands::Config { init, path, json } => {
            commands::config::execute(commands::config::ConfigArgs {
                init,
                path,
                json,
                silent,
                config_file: cli.config_file,
            })
        }
        Commands::Benchmark {
            target,
            size,
            block_size,
            pattern,
            passes,
            test_block_sizes,
            json,
            yes,
        } => {
            let effective_skip_confirm = yes || silent || settings.behavior.skip_confirmation;

            // Apply settings as defaults when CLI options are not explicitly set
            let effective_block_size =
                block_size.unwrap_or_else(|| settings.benchmark.block_size.clone());
            let effective_pattern = pattern.unwrap_or_else(|| settings.benchmark.pattern.clone());
            let effective_passes = passes.unwrap_or(settings.benchmark.passes);
            // For test_size: use CLI value if provided, otherwise use config default
            // (but only when not using --test-block-sizes)
            let effective_test_size = size.or_else(|| {
                if test_block_sizes.is_none() {
                    Some(settings.benchmark.test_size.clone())
                } else {
                    None
                }
            });
            // JSON output: CLI flag overrides config
            let effective_json = json || settings.benchmark.json;

            commands::benchmark::execute(commands::benchmark::BenchmarkArgs {
                target,
                test_size: effective_test_size,
                block_size: effective_block_size,
                pattern: effective_pattern,
                passes: effective_passes,
                json: effective_json,
                skip_confirm: effective_skip_confirm,
                silent,
                test_block_sizes,
                cancel_flag: running,
            })
        }
    }
}
