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
use std::io::IsTerminal;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[macro_use]
mod macros;
mod commands;
mod progress;
mod prompt;

/// Engraver - A safe, fast tool for creating bootable USB drives
#[derive(Parser)]
#[command(name = "engraver")]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Hide progress and status output (results, errors and confirmation prompts still print)
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Like --quiet, and also hide the drive list, configuration and benchmark results, skip confirmation prompts (implies --yes) and hide the Ctrl+C messages
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

        /// Block size for writing (e.g., 4M, 1M, 512K). Must be a power of 2 from 4K to 64M. Default from config or 32M
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

        /// Block size for reading (e.g., 4M, 1M, 512K). Must be a power of 2 from 4K to 64M
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

    /// Erase a drive by zero-filling the entire device
    Erase {
        /// Target device (e.g., /dev/sdb, /dev/disk2, \\.\PhysicalDrive1)
        target: String,

        /// Skip confirmation prompt (use with caution!)
        #[arg(short = 'y', long)]
        yes: bool,

        /// Block size for writing (e.g., 4M, 1M, 512K). Must be a power of 2 from 4K to 64M. Default from config or 32M
        #[arg(short, long)]
        block_size: Option<String>,

        /// Force erase even on system drives (DANGEROUS!)
        #[arg(long, hide = true)]
        force: bool,

        /// Do not unmount partitions before erasing
        #[arg(long)]
        no_unmount: bool,
    },

    /// Benchmark write speed of a drive (DESTRUCTIVE)
    Benchmark {
        /// Target device (e.g., /dev/sdb, \\.\PhysicalDrive1)
        target: String,

        /// Amount of data to write per pass, exactly (e.g., 256M, 1G). Must hold at least one block. Mutually exclusive with --test-block-sizes. Default from config or 256M, raised to ten blocks of the largest block size when smaller
        #[arg(short, long)]
        size: Option<String>,

        /// Block size for writing (e.g., 4K, 1M, 4M). Must be power of 2, max 64M. Default from config or 32M
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

/// How much a run prints, from `--quiet`, `--silent` and `quiet` in the
/// configuration file.
///
/// Four decisions hang on it, and `-q` makes only the first: hide the progress
/// and status output, hide a command's result, answer yes to every confirmation
/// prompt, and hide the Ctrl+C messages. Each has a method, so a caller asks for
/// the one it means.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputMode {
    /// Everything prints.
    Normal,
    /// `-q`, or `quiet = true` in the configuration file.
    Quiet,
    /// `--silent`.
    Silent,
}

impl OutputMode {
    fn new(cli: &Cli, settings: &Settings) -> Self {
        if cli.silent {
            Self::Silent
        } else if cli.quiet || settings.behavior.quiet {
            Self::Quiet
        } else {
            Self::Normal
        }
    }

    /// Hide the logs, and the progress and status lines each command prints.
    fn hides_status(self) -> bool {
        self != Self::Normal
    }

    /// Hide what a command is run to show: the drive list, the configuration,
    /// a benchmark's results. `-q` keeps them, so `$(engraver config --path)`
    /// still works with `quiet = true` in the configuration file.
    fn hides_results(self) -> bool {
        self == Self::Silent
    }

    /// Answer yes to every confirmation prompt.
    fn skips_confirmation(self) -> bool {
        self == Self::Silent
    }

    /// Hide the messages that a Ctrl+C prints.
    fn hides_cancel_notices(self) -> bool {
        self == Self::Silent
    }
}

fn main() {
    // Set up panic handler for nicer error messages
    std::panic::set_hook(Box::new(|panic_info| {
        eprintln!("{} {}", style("Error:").red().bold(), panic_info);
    }));

    if let Err(e) = run() {
        let exit = exit_for(&e);

        if let Some(line) = &exit.cancelled_line {
            eprintln!("{line}");
        } else {
            eprintln!("{} {}", style("Error:").red().bold(), e);

            // Show cause chain in verbose mode
            if std::env::var("RUST_BACKTRACE").is_ok() {
                let mut source = e.source();
                while let Some(cause) = source {
                    eprintln!("  {} {}", style("Caused by:").yellow(), cause);
                    source = cause.source();
                }
            }
        }

        std::process::exit(exit.code);
    }
}

/// How a failed run ends: the exit status, and for a cancelled run the one
/// line to print in place of the `Error:` report.
struct Exit {
    code: i32,
    cancelled_line: Option<String>,
}

/// Exit status for a run cancelled with Ctrl+C: 128 + SIGINT.
const EXIT_CANCELLED: i32 = 130;

/// Decide how a failed run ends.
///
/// A run cancelled with Ctrl+C exits 130, so `engraver ... && next-step`
/// stops and a supervisor sees the interrupt. That is the case when any error
/// in the chain is [`engraver_core::Error::Cancelled`]. Every other error
/// exits 1.
fn exit_for(err: &anyhow::Error) -> Exit {
    if !commands::is_cancelled(err) {
        return Exit {
            code: 1,
            cancelled_line: None,
        };
    }

    // The outermost message says what was cancelled. A bare `Cancelled` has
    // only the library's wording, so use a plain default instead.
    let line = if err.chain().len() > 1 {
        err.to_string()
    } else {
        "Cancelled.".to_string()
    };
    Exit {
        code: EXIT_CANCELLED,
        cancelled_line: Some(line),
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    let (settings, settings_error) = match Settings::try_load_from_path(
        cli.config_file.clone().or_else(Settings::config_path),
    ) {
        Ok(settings) => (settings, None),
        Err(e) => (Settings::default(), Some(e)),
    };

    let output = OutputMode::new(&cli, &settings);

    // Initialize logging
    // --quiet and --silent turn off logs entirely, not even errors to tracing
    let filter = if cli.verbose {
        EnvFilter::new("debug")
    } else if output.hides_status() {
        EnvFilter::new("off")
    } else {
        EnvFilter::new("info")
    };

    // Logs go to stderr so stdout carries only a command's own output, such
    // as the document that `--json` prints. Colour only when a person is
    // reading stderr. `with_ansi` overrides tracing-subscriber's own NO_COLOR
    // check, so the check is repeated here.
    let ansi = std::io::stderr().is_terminal()
        && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty());

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(ansi)
        .with_target(false)
        .without_time()
        .init();

    if let Some(e) = settings_error {
        tracing::warn!("{e}\nUsing default settings.");
    }

    // What each command's `silent` field means: hide its progress and status
    // output. Confirmation prompts and Ctrl+C messages have their own switches.
    let silent = output.hides_status();

    // Set up Ctrl+C handler (suppress messages in silent mode).
    //
    // Convention: `true = cancel requested`. This matches the writer,
    // verifier, and benchmark cancel handles in engraver-core, so the
    // same Arc can be passed straight through via `.with_cancel_flag()`
    // without an intermediate polling thread to invert the meaning.
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let c = cancel.clone();
    let hide_cancel_notices = output.hides_cancel_notices();
    ctrlc::set_handler(move || {
        if c.load(std::sync::atomic::Ordering::SeqCst) {
            // Second Ctrl+C — already cancelled once; force exit.
            if !hide_cancel_notices {
                eprintln!("\n{}", style("Forced exit").red().bold());
            }
            std::process::exit(EXIT_CANCELLED);
        }
        c.store(true, std::sync::atomic::Ordering::SeqCst);
        if !hide_cancel_notices {
            eprintln!(
                "\n{}",
                style("Cancelling... Press Ctrl+C again to force exit").yellow()
            );
        }
    })?;

    match cli.command {
        Commands::List { all, json } => commands::list::execute(all, json, output.hides_results()),
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
            let effective_skip_confirm =
                yes || output.skips_confirmation() || settings.behavior.skip_confirmation;
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
                cancel_flag: cancel,
                silent,
                resume,
                checkpoint: effective_checkpoint,
                auto_checksum: effective_auto_checksum,
                show_partitions,
            })
        }
        Commands::Erase {
            target,
            yes,
            block_size,
            force,
            no_unmount,
        } => {
            let effective_block_size =
                block_size.unwrap_or_else(|| settings.write.block_size.clone());
            let effective_skip_confirm =
                yes || output.skips_confirmation() || settings.behavior.skip_confirmation;

            commands::erase::execute(commands::erase::EraseArgs {
                target,
                skip_confirm: effective_skip_confirm,
                block_size: effective_block_size,
                force,
                no_unmount,
                cancel_flag: cancel,
                silent,
            })
        }
        Commands::Verify {
            source,
            target,
            block_size,
        } => commands::verify::execute(&source, &target, &block_size, cancel, silent),
        Commands::Checksum { source, algorithm } => {
            let effective_algorithm =
                algorithm.unwrap_or_else(|| settings.checksum.algorithm.clone());
            commands::checksum::execute(&source, &effective_algorithm, cancel, silent)
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
                silent: output.hides_results(),
                settings,
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
            let effective_skip_confirm =
                yes || output.skips_confirmation() || settings.behavior.skip_confirmation;

            // Apply settings as defaults when CLI options are not explicitly set
            let effective_block_size =
                block_size.unwrap_or_else(|| settings.benchmark.block_size.clone());
            let effective_pattern = pattern.unwrap_or_else(|| settings.benchmark.pattern.clone());
            let effective_passes = passes.unwrap_or(settings.benchmark.passes);
            let effective_test_size = match size {
                Some(size) => commands::benchmark::TestSize::Exact(size),
                None => {
                    commands::benchmark::TestSize::Default(settings.benchmark.test_size.clone())
                }
            };
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
                hide_results: output.hides_results(),
                test_block_sizes,
                cancel_flag: cancel,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use engraver_core::Error;

    #[test]
    fn test_exit_for_cancelled_run_is_130_with_the_outer_message() {
        let err = anyhow::Error::new(Error::Cancelled)
            .context("Failed to calculate checksum")
            .context("Checksum cancelled.");

        let exit = exit_for(&err);

        assert_eq!(exit.code, 130);
        assert_eq!(exit.cancelled_line.as_deref(), Some("Checksum cancelled."));
    }

    #[test]
    fn test_exit_for_bare_cancelled_run_is_130_with_a_default_message() {
        let err = anyhow::Error::new(Error::Cancelled);

        let exit = exit_for(&err);

        assert_eq!(exit.code, 130);
        assert_eq!(exit.cancelled_line.as_deref(), Some("Cancelled."));
    }

    #[test]
    fn test_exit_for_plain_error_is_1_and_prints_as_an_error() {
        let err = anyhow::anyhow!("disk exploded").context("Failed to write");

        let exit = exit_for(&err);

        assert_eq!(exit.code, 1);
        assert_eq!(exit.cancelled_line, None);
    }

    #[test]
    fn test_exit_for_other_core_error_under_context_is_1() {
        let err = anyhow::Error::new(Error::SourceNotFound("a.iso".to_string()))
            .context("Failed to validate source");

        let exit = exit_for(&err);

        assert_eq!(exit.code, 1);
        assert_eq!(exit.cancelled_line, None);
    }

    fn prompt_failure(kind: std::io::ErrorKind, message: &str) -> anyhow::Error {
        commands::into_answer(Err(std::io::Error::new(kind, message))).unwrap_err()
    }

    #[test]
    fn test_exit_for_ctrl_c_at_a_prompt_is_130_with_one_cancel_line() {
        let err = commands::into_answer(Ok(None)).unwrap_err();

        let exit = exit_for(&err);

        assert_eq!(exit.code, 130);
        assert_eq!(exit.cancelled_line.as_deref(), Some("Cancelled."));
    }

    #[test]
    fn test_exit_for_a_prompt_without_a_terminal_is_1_and_prints_as_an_error() {
        let err = prompt_failure(std::io::ErrorKind::NotConnected, "not a terminal");

        let exit = exit_for(&err);

        assert_eq!(exit.code, 1);
        assert_eq!(exit.cancelled_line, None);
    }

    #[test]
    fn test_exit_for_a_prompt_whose_input_ended_is_1() {
        let err = prompt_failure(std::io::ErrorKind::UnexpectedEof, "Reached end of file");

        let exit = exit_for(&err);

        assert_eq!(exit.code, 1);
        assert_eq!(exit.cancelled_line, None);
    }

    /// The mode a command line gets, with `quiet` as the configuration file sets it.
    fn mode(args: &[&str], config_quiet: bool) -> OutputMode {
        let cli = Cli::try_parse_from(args).unwrap();
        let mut settings = Settings::default();
        settings.behavior.quiet = config_quiet;
        OutputMode::new(&cli, &settings)
    }

    #[test]
    fn test_output_mode_without_a_flag_hides_nothing() {
        let mode = mode(&["engraver", "list"], false);

        assert_eq!(mode, OutputMode::Normal);
        assert!(!mode.hides_status());
        assert!(!mode.hides_results());
        assert!(!mode.skips_confirmation());
        assert!(!mode.hides_cancel_notices());
    }

    #[test]
    fn test_output_mode_quiet_hides_status_but_still_asks_and_still_reports_a_cancel() {
        // The flag is global, so it works on either side of the subcommand,
        // and the configuration file turns on the same mode.
        for mode in [
            mode(&["engraver", "-q", "list"], false),
            mode(&["engraver", "list", "--quiet"], false),
            mode(&["engraver", "list"], true),
        ] {
            assert_eq!(mode, OutputMode::Quiet);
            assert!(mode.hides_status());
            assert!(!mode.hides_results());
            assert!(!mode.skips_confirmation());
            assert!(!mode.hides_cancel_notices());
        }
    }

    #[test]
    fn test_output_mode_silent_also_skips_confirmation_and_hides_cancel_notices() {
        for mode in [
            mode(&["engraver", "--silent", "list"], false),
            mode(&["engraver", "-q", "--silent", "list"], false),
            mode(&["engraver", "--silent", "list"], true),
        ] {
            assert_eq!(mode, OutputMode::Silent);
            assert!(mode.hides_status());
            assert!(mode.hides_results());
            assert!(mode.skips_confirmation());
            assert!(mode.hides_cancel_notices());
        }
    }
}
