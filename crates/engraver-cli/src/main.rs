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

use anyhow::{anyhow, Result};
use clap::error::ErrorKind;
use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{generate, Shell};
use clap_mangen::Man;
use console::style;
use engraver_core::{Settings, SourceOptions};
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;
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
    #[arg(short, long, global = true, overrides_with = "no_quiet")]
    quiet: bool,

    /// Show progress and status output, even if the config file sets quiet
    #[arg(long, global = true, overrides_with = "quiet")]
    no_quiet: bool,

    /// Like --quiet, and also hide the human-readable results, skip confirmation prompts (implies --yes) and hide the Ctrl+C messages. JSON, checksum hashes and config --path still print
    #[arg(long, global = true)]
    silent: bool,

    /// Use a custom configuration file instead of the default
    #[arg(long, global = true, value_name = "PATH")]
    config_file: Option<PathBuf>,

    #[command(subcommand)]
    command: Commands,
}

impl Cli {
    /// Parse a command line like `Cli::try_parse_from`, then reject `--silent`
    /// together with `--confirm`.
    ///
    /// `--silent` answers yes to every prompt, which `--confirm` contradicts.
    /// clap can't report this itself. `--silent` is global, and a global flag
    /// given before the subcommand isn't visible to the subcommand's own
    /// conflict check. So the check is here, and it reports the error the way
    /// clap does, with the same wording and exit status 2.
    fn try_parse_checked<I, T>(args: I) -> Result<Self, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        let cli = Self::try_parse_from(args)?;
        if cli.silent && cli.command.asks_for_confirmation() {
            return Err(Self::command().error(
                ErrorKind::ArgumentConflict,
                "the argument '--confirm' cannot be used with '--silent'",
            ));
        }
        Ok(cli)
    }
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
    Write(WriteOpts),

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
    Erase(EraseOpts),

    /// Benchmark write speed of a drive (DESTRUCTIVE)
    Benchmark(BenchmarkOpts),
}

impl Commands {
    /// Whether the command line has `--confirm`, which only the commands that
    /// ask for confirmation take.
    fn asks_for_confirmation(&self) -> bool {
        match self {
            Self::Write(opts) => opts.confirm,
            Self::Erase(opts) => opts.confirm,
            Self::Benchmark(opts) => opts.confirm,
            _ => false,
        }
    }
}

/// The value of an on/off setting for this run.
///
/// Each setting that the configuration file turns on has a flag that turns it
/// on and a flag that turns it off. The flag that was given decides, and the
/// setting decides when neither was.
///
/// The two flags override each other in clap, so at most one is set, and it is
/// the last one on the command line. The exception is a global flag, such as
/// `--quiet` and `--no-quiet`, given on both sides of the subcommand. clap
/// checks each side alone and sets both, so the off flag wins.
fn flag_or_setting(on: bool, off: bool, setting: bool) -> bool {
    let flag = match (on, off) {
        (_, true) => Some(false),
        (true, false) => Some(true),
        (false, false) => None,
    };
    flag.unwrap_or(setting)
}

/// Parse the settings that shape how a source is read.
///
/// Only the commands that open a source call this, so a value that can't be
/// used stops them and leaves `list` and `config` alone. The message names
/// the setting and the config file.
fn source_options(settings: &Settings, config_file: Option<&Path>) -> Result<SourceOptions> {
    settings.source_options().map_err(|e| match config_file {
        Some(path) => anyhow!("Invalid setting in {}: {e}", path.display()),
        None => anyhow!("Invalid setting: {e}"),
    })
}

/// The options of `engraver write`.
#[derive(clap::Args)]
struct WriteOpts {
    /// Source image (local file or URL)
    source: String,

    /// Target device (e.g., /dev/sdb, /dev/disk2, \\.\PhysicalDrive1)
    target: String,

    /// Verify write by reading back and comparing (can be set in config)
    #[arg(long, overrides_with = "no_verify")]
    verify: bool,

    /// Don't verify, even if the config file sets verify
    #[arg(long, overrides_with = "verify")]
    no_verify: bool,

    /// Skip confirmation prompt (use with caution!)
    #[arg(short = 'y', long, overrides_with = "confirm")]
    yes: bool,

    /// Ask for confirmation, even if the config file sets skip_confirmation. Can't be used with --silent
    #[arg(long, overrides_with = "yes")]
    confirm: bool,

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
    #[arg(long, overrides_with = "no_checkpoint")]
    checkpoint: bool,

    /// Don't checkpoint, even if the config file sets checkpoint. Can't be used with --resume
    #[arg(long, overrides_with = "checkpoint", conflicts_with = "resume")]
    no_checkpoint: bool,

    /// Automatically detect and verify checksum from .sha256, .sha512, .md5 files
    #[arg(long, overrides_with = "no_auto_checksum")]
    auto_checksum: bool,

    /// Don't look for a checksum file, even if the config file sets auto_detect
    #[arg(long, overrides_with = "auto_checksum")]
    no_auto_checksum: bool,

    /// Show partition layout of source image before writing
    #[arg(long, short = 'p')]
    show_partitions: bool,
}

impl WriteOpts {
    /// Apply the configuration file's settings to what the command line left
    /// unsaid, and give `commands::write` its arguments. `source_options` is
    /// the settings' [`source_options`], parsed.
    fn resolve(
        self,
        settings: &Settings,
        source_options: SourceOptions,
        output: OutputMode,
        cancel: Arc<AtomicBool>,
    ) -> commands::write::WriteArgs {
        commands::write::WriteArgs {
            source: self.source,
            target: self.target,
            verify: flag_or_setting(self.verify, self.no_verify, settings.write.verify),
            skip_confirm: output.skips_confirmation()
                || flag_or_setting(self.yes, self.confirm, settings.behavior.skip_confirmation),
            block_size: self
                .block_size
                .unwrap_or_else(|| settings.write.block_size.clone()),
            checksum: self.checksum,
            checksum_algo: self
                .checksum_algo
                .unwrap_or_else(|| settings.checksum.algorithm.clone()),
            force: self.force,
            no_unmount: self.no_unmount,
            cancel_flag: cancel,
            silent: output.hides_status(),
            resume: self.resume,
            checkpoint: self.resume
                || flag_or_setting(
                    self.checkpoint,
                    self.no_checkpoint,
                    settings.write.checkpoint,
                ),
            auto_checksum: flag_or_setting(
                self.auto_checksum,
                self.no_auto_checksum,
                settings.checksum.auto_detect,
            ),
            show_partitions: self.show_partitions,
            source_options,
            retry_attempts: settings.write.retry_attempts,
            retry_delay: Duration::from_millis(settings.write.retry_delay_ms),
        }
    }
}

/// The options of `engraver erase`.
#[derive(clap::Args)]
struct EraseOpts {
    /// Target device (e.g., /dev/sdb, /dev/disk2, \\.\PhysicalDrive1)
    target: String,

    /// Skip confirmation prompt (use with caution!)
    #[arg(short = 'y', long, overrides_with = "confirm")]
    yes: bool,

    /// Ask for confirmation, even if the config file sets skip_confirmation. Can't be used with --silent
    #[arg(long, overrides_with = "yes")]
    confirm: bool,

    /// Block size for writing (e.g., 4M, 1M, 512K). Must be a power of 2 from 4K to 64M. Default from config or 32M
    #[arg(short, long)]
    block_size: Option<String>,

    /// Force erase even on system drives (DANGEROUS!)
    #[arg(long, hide = true)]
    force: bool,

    /// Do not unmount partitions before erasing
    #[arg(long)]
    no_unmount: bool,
}

impl EraseOpts {
    /// Apply the configuration file's settings to what the command line left
    /// unsaid, and give `commands::erase` its arguments.
    fn resolve(
        self,
        settings: &Settings,
        output: OutputMode,
        cancel: Arc<AtomicBool>,
    ) -> commands::erase::EraseArgs {
        commands::erase::EraseArgs {
            target: self.target,
            skip_confirm: output.skips_confirmation()
                || flag_or_setting(self.yes, self.confirm, settings.behavior.skip_confirmation),
            block_size: self
                .block_size
                .unwrap_or_else(|| settings.write.block_size.clone()),
            force: self.force,
            no_unmount: self.no_unmount,
            cancel_flag: cancel,
            silent: output.hides_status(),
        }
    }
}

/// The options of `engraver benchmark`.
#[derive(clap::Args)]
struct BenchmarkOpts {
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
    #[arg(long, overrides_with = "no_json")]
    json: bool,

    /// Print the results as text, even if the config file sets json
    #[arg(long, overrides_with = "json")]
    no_json: bool,

    /// Skip confirmation prompt (DANGEROUS!)
    #[arg(short = 'y', long, overrides_with = "confirm")]
    yes: bool,

    /// Ask for confirmation, even if the config file sets skip_confirmation. Can't be used with --silent
    #[arg(long, overrides_with = "yes")]
    confirm: bool,
}

impl BenchmarkOpts {
    /// Apply the configuration file's settings to what the command line left
    /// unsaid, and give `commands::benchmark` its arguments.
    fn resolve(
        self,
        settings: &Settings,
        output: OutputMode,
        cancel: Arc<AtomicBool>,
    ) -> commands::benchmark::BenchmarkArgs {
        commands::benchmark::BenchmarkArgs {
            target: self.target,
            test_size: match self.size {
                Some(size) => commands::benchmark::TestSize::Exact(size),
                None => {
                    commands::benchmark::TestSize::Default(settings.benchmark.test_size.clone())
                }
            },
            block_size: self
                .block_size
                .unwrap_or_else(|| settings.benchmark.block_size.clone()),
            pattern: self
                .pattern
                .unwrap_or_else(|| settings.benchmark.pattern.clone()),
            passes: self.passes.unwrap_or(settings.benchmark.passes),
            json: flag_or_setting(self.json, self.no_json, settings.benchmark.json),
            skip_confirm: output.skips_confirmation()
                || flag_or_setting(self.yes, self.confirm, settings.behavior.skip_confirmation),
            silent: output.hides_status(),
            hide_results: output.hides_results(),
            test_block_sizes: self.test_block_sizes,
            cancel_flag: cancel,
        }
    }
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
        } else if flag_or_setting(cli.quiet, cli.no_quiet, settings.behavior.quiet) {
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
    let cli = Cli::try_parse_checked(std::env::args_os()).unwrap_or_else(|e| e.exit());

    let config_path = cli.config_file.clone().or_else(Settings::config_path);
    let (settings, settings_error) = match Settings::try_load_from_path(config_path.clone()) {
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
        Commands::Write(opts) => {
            let source_options = source_options(&settings, config_path.as_deref())?;
            commands::write::execute(opts.resolve(&settings, source_options, output, cancel))
        }
        Commands::Erase(opts) => commands::erase::execute(opts.resolve(&settings, output, cancel)),
        Commands::Verify {
            source,
            target,
            block_size,
        } => {
            let source_options = source_options(&settings, config_path.as_deref())?;
            commands::verify::execute(
                &source,
                &target,
                &block_size,
                &source_options,
                cancel,
                silent,
            )
        }
        Commands::Checksum { source, algorithm } => {
            let source_options = source_options(&settings, config_path.as_deref())?;
            let effective_algorithm =
                algorithm.unwrap_or_else(|| settings.checksum.algorithm.clone());
            commands::checksum::execute(
                &source,
                &effective_algorithm,
                &source_options,
                cancel,
                silent,
            )
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
        Commands::Benchmark(opts) => {
            commands::benchmark::execute(opts.resolve(&settings, output, cancel))
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
        let cli = Cli::try_parse_checked(args).unwrap();
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

    fn no_cancel() -> Arc<AtomicBool> {
        Arc::new(AtomicBool::new(false))
    }

    /// What `engraver write` receives for a command line, with the settings
    /// of the configuration file.
    fn write_args(
        args: &[&str],
        settings: &Settings,
    ) -> Result<commands::write::WriteArgs, clap::Error> {
        let cli = Cli::try_parse_checked(args)?;
        let output = OutputMode::new(&cli, settings);
        match cli.command {
            Commands::Write(opts) => {
                Ok(opts.resolve(settings, SourceOptions::default(), output, no_cancel()))
            }
            _ => panic!("not an `engraver write` command line: {args:?}"),
        }
    }

    /// What `engraver erase` receives for a command line.
    fn erase_args(
        args: &[&str],
        settings: &Settings,
    ) -> Result<commands::erase::EraseArgs, clap::Error> {
        let cli = Cli::try_parse_checked(args)?;
        let output = OutputMode::new(&cli, settings);
        match cli.command {
            Commands::Erase(opts) => Ok(opts.resolve(settings, output, no_cancel())),
            _ => panic!("not an `engraver erase` command line: {args:?}"),
        }
    }

    /// What `engraver benchmark` receives for a command line.
    fn benchmark_args(
        args: &[&str],
        settings: &Settings,
    ) -> Result<commands::benchmark::BenchmarkArgs, clap::Error> {
        let cli = Cli::try_parse_checked(args)?;
        let output = OutputMode::new(&cli, settings);
        match cli.command {
            Commands::Benchmark(opts) => Ok(opts.resolve(settings, output, no_cancel())),
            _ => panic!("not an `engraver benchmark` command line: {args:?}"),
        }
    }

    /// `command` with `flags` after its last argument.
    fn with_flags<'a>(command: &[&'a str], flags: &[&'a str]) -> Vec<&'a str> {
        [command, flags].concat()
    }

    /// `command` with a global `flag` between the program name and the subcommand.
    fn before_subcommand<'a>(command: &[&'a str], flag: &'a str) -> Vec<&'a str> {
        [&command[..1], &[flag], &command[1..]].concat()
    }

    /// The kind of usage error a command line gives, or `None` if it parses.
    fn usage_error(args: &[&str]) -> Option<ErrorKind> {
        Cli::try_parse_checked(args).err().map(|e| e.kind())
    }

    const WRITE: &[&str] = &["engraver", "write", "a.iso", "/dev/sdb"];
    const ERASE: &[&str] = &["engraver", "erase", "/dev/sdb"];
    const BENCHMARK: &[&str] = &["engraver", "benchmark", "/dev/sdb"];

    /// One on/off setting of the configuration file, with the flag that turns
    /// it on for one run and the flag that turns it off.
    struct Switch {
        /// A command line without the flags, to which the flags are appended.
        command: &'static [&'static str],
        on: &'static str,
        off: &'static str,
        set: fn(&mut Settings, bool),
        /// The value the command receives for this setting.
        received: fn(&[&str], &Settings) -> Result<bool, clap::Error>,
    }

    const VERIFY: Switch = Switch {
        command: WRITE,
        on: "--verify",
        off: "--no-verify",
        set: |settings, on| settings.write.verify = on,
        received: |args, settings| Ok(write_args(args, settings)?.verify),
    };

    const CHECKPOINT: Switch = Switch {
        command: WRITE,
        on: "--checkpoint",
        off: "--no-checkpoint",
        set: |settings, on| settings.write.checkpoint = on,
        received: |args, settings| Ok(write_args(args, settings)?.checkpoint),
    };

    const AUTO_CHECKSUM: Switch = Switch {
        command: WRITE,
        on: "--auto-checksum",
        off: "--no-auto-checksum",
        set: |settings, on| settings.checksum.auto_detect = on,
        received: |args, settings| Ok(write_args(args, settings)?.auto_checksum),
    };

    const WRITE_SKIP_CONFIRMATION: Switch = Switch {
        command: WRITE,
        on: "--yes",
        off: "--confirm",
        set: |settings, on| settings.behavior.skip_confirmation = on,
        received: |args, settings| Ok(write_args(args, settings)?.skip_confirm),
    };

    const ERASE_SKIP_CONFIRMATION: Switch = Switch {
        command: ERASE,
        on: "--yes",
        off: "--confirm",
        set: |settings, on| settings.behavior.skip_confirmation = on,
        received: |args, settings| Ok(erase_args(args, settings)?.skip_confirm),
    };

    const BENCHMARK_SKIP_CONFIRMATION: Switch = Switch {
        command: BENCHMARK,
        on: "--yes",
        off: "--confirm",
        set: |settings, on| settings.behavior.skip_confirmation = on,
        received: |args, settings| Ok(benchmark_args(args, settings)?.skip_confirm),
    };

    const BENCHMARK_JSON: Switch = Switch {
        command: BENCHMARK,
        on: "--json",
        off: "--no-json",
        set: |settings, on| settings.benchmark.json = on,
        received: |args, settings| Ok(benchmark_args(args, settings)?.json),
    };

    const QUIET: Switch = Switch {
        command: &["engraver", "list"],
        on: "--quiet",
        off: "--no-quiet",
        set: |settings, on| settings.behavior.quiet = on,
        received: |args, settings| {
            let cli = Cli::try_parse_checked(args)?;
            Ok(OutputMode::new(&cli, settings).hides_status())
        },
    };

    /// With the setting on or off, a run gets the setting when no flag is
    /// given, the flag's value when one is, and the last flag's value when
    /// both are. A flag that doesn't parse or a wrong value fails the test,
    /// and the failure lists every case that went wrong.
    fn check_switch(switch: &Switch) {
        let (on, off) = (switch.on, switch.off);
        let mut wrong = Vec::new();

        for setting in [false, true] {
            let mut settings = Settings::default();
            (switch.set)(&mut settings, setting);

            let cases: [(&[&str], bool); 5] = [
                (&[], setting),
                (&[on], true),
                (&[off], false),
                (&[on, off], false),
                (&[off, on], true),
            ];
            for (flags, expected) in cases {
                let args: Vec<&str> = switch.command.iter().chain(flags).copied().collect();
                match (switch.received)(&args, &settings) {
                    Ok(got) if got == expected => {}
                    Ok(got) => wrong.push(format!(
                        "setting {setting}, `{}`: got {got}, expected {expected}",
                        args[1..].join(" ")
                    )),
                    Err(e) => wrong.push(format!(
                        "setting {setting}, `{}`: {}",
                        args[1..].join(" "),
                        e.to_string().lines().next().unwrap_or_default()
                    )),
                }
            }
        }

        assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
    }

    #[test]
    fn test_verify_flags_override_the_verify_setting() {
        check_switch(&VERIFY);
    }

    #[test]
    fn test_checkpoint_flags_override_the_checkpoint_setting() {
        check_switch(&CHECKPOINT);
    }

    #[test]
    fn test_auto_checksum_flags_override_the_auto_detect_setting() {
        check_switch(&AUTO_CHECKSUM);
    }

    #[test]
    fn test_write_confirm_flags_override_the_skip_confirmation_setting() {
        check_switch(&WRITE_SKIP_CONFIRMATION);
    }

    #[test]
    fn test_erase_confirm_flags_override_the_skip_confirmation_setting() {
        check_switch(&ERASE_SKIP_CONFIRMATION);
    }

    #[test]
    fn test_benchmark_confirm_flags_override_the_skip_confirmation_setting() {
        check_switch(&BENCHMARK_SKIP_CONFIRMATION);
    }

    #[test]
    fn test_json_flags_override_the_benchmark_json_setting() {
        check_switch(&BENCHMARK_JSON);
    }

    #[test]
    fn test_quiet_flags_override_the_quiet_setting() {
        check_switch(&QUIET);
    }

    #[test]
    fn test_confirm_brings_back_the_prompt_that_skip_confirmation_turned_off() {
        let mut settings = Settings::default();
        settings.behavior.skip_confirmation = true;

        assert!(write_args(WRITE, &settings).unwrap().skip_confirm);
        assert!(erase_args(ERASE, &settings).unwrap().skip_confirm);
        assert!(benchmark_args(BENCHMARK, &settings).unwrap().skip_confirm);

        assert!(
            !write_args(&with_flags(WRITE, &["--confirm"]), &settings)
                .unwrap()
                .skip_confirm
        );
        assert!(
            !erase_args(&with_flags(ERASE, &["--confirm"]), &settings)
                .unwrap()
                .skip_confirm
        );
        assert!(
            !benchmark_args(&with_flags(BENCHMARK, &["--confirm"]), &settings)
                .unwrap()
                .skip_confirm
        );
    }

    #[test]
    fn test_short_y_is_the_yes_flag_and_confirm_overrides_it_in_an_alias() {
        // `alias ew='engraver write -y'`, then `ew ... --confirm`.
        let settings = Settings::default();

        assert!(
            write_args(&with_flags(WRITE, &["-y"]), &settings)
                .unwrap()
                .skip_confirm
        );
        assert!(
            !write_args(&with_flags(WRITE, &["-y", "--confirm"]), &settings)
                .unwrap()
                .skip_confirm
        );
    }

    #[test]
    fn test_resume_turns_checkpointing_on_whatever_the_setting_says() {
        for setting in [false, true] {
            let mut settings = Settings::default();
            settings.write.checkpoint = setting;

            let args = write_args(&with_flags(WRITE, &["--resume"]), &settings).unwrap();

            assert!(args.resume);
            assert!(args.checkpoint);
        }
    }

    #[test]
    fn test_write_takes_its_retries_from_the_settings() {
        let mut settings = Settings::default();
        settings.write.retry_attempts = 7;
        settings.write.retry_delay_ms = 250;

        let args = write_args(WRITE, &settings).unwrap();

        assert_eq!(args.retry_attempts, 7);
        assert_eq!(args.retry_delay, Duration::from_millis(250));
    }

    #[test]
    fn test_source_options_error_names_the_setting_and_the_config_file() {
        let mut settings = Settings::default();
        settings.write.read_buffer_size = "banana".to_string();

        let err =
            source_options(&settings, Some(Path::new("/etc/engraver_config.toml"))).unwrap_err();

        assert_eq!(
            err.to_string(),
            r#"Invalid setting in /etc/engraver_config.toml: write.read_buffer_size = "banana" is not a size, such as "4M""#
        );
    }

    #[test]
    fn test_resume_with_no_checkpoint_is_a_usage_error() {
        for flags in [
            ["--resume", "--no-checkpoint"],
            ["--no-checkpoint", "--resume"],
        ] {
            assert_eq!(
                usage_error(&with_flags(WRITE, &flags)),
                Some(ErrorKind::ArgumentConflict),
                "{flags:?}"
            );
        }
    }

    #[test]
    fn test_silent_with_confirm_is_a_usage_error() {
        // `--silent` is global, so it can come before or after the subcommand.
        for command in [WRITE, ERASE, BENCHMARK] {
            for args in [
                with_flags(&before_subcommand(command, "--silent"), &["--confirm"]),
                with_flags(command, &["--silent", "--confirm"]),
                with_flags(command, &["--confirm", "--silent"]),
            ] {
                assert_eq!(
                    usage_error(&args),
                    Some(ErrorKind::ArgumentConflict),
                    "{args:?}"
                );
            }
        }
    }

    #[test]
    fn test_silent_still_skips_confirmation_when_the_setting_is_off() {
        let settings = Settings::default();

        assert!(
            write_args(&before_subcommand(WRITE, "--silent"), &settings)
                .unwrap()
                .skip_confirm
        );
        assert!(
            erase_args(&before_subcommand(ERASE, "--silent"), &settings)
                .unwrap()
                .skip_confirm
        );
        assert!(
            benchmark_args(&before_subcommand(BENCHMARK, "--silent"), &settings)
                .unwrap()
                .skip_confirm
        );
    }

    #[test]
    fn test_no_quiet_cancels_only_the_quiet_setting_so_silent_still_wins() {
        let mut settings = Settings::default();
        settings.behavior.quiet = true;

        for args in [
            ["engraver", "--silent", "--no-quiet", "list"],
            ["engraver", "--no-quiet", "--silent", "list"],
        ] {
            let cli = Cli::try_parse_checked(args).unwrap();
            assert_eq!(OutputMode::new(&cli, &settings), OutputMode::Silent);
        }
    }

    #[test]
    fn test_no_quiet_is_global_so_it_works_on_either_side_of_the_subcommand() {
        let mut settings = Settings::default();
        settings.behavior.quiet = true;

        for args in [
            ["engraver", "--no-quiet", "list"],
            ["engraver", "list", "--no-quiet"],
        ] {
            let cli = Cli::try_parse_checked(args).unwrap();
            assert_eq!(OutputMode::new(&cli, &settings), OutputMode::Normal);
        }
    }

    #[test]
    fn test_no_quiet_wins_over_quiet_on_the_other_side_of_the_subcommand() {
        // clap checks the two sides of the subcommand apart, so it sets both
        // flags and can't say which came last.
        for args in [
            ["engraver", "-q", "list", "--no-quiet"],
            ["engraver", "--no-quiet", "list", "-q"],
        ] {
            let cli = Cli::try_parse_checked(args).unwrap();
            assert_eq!(
                OutputMode::new(&cli, &Settings::default()),
                OutputMode::Normal
            );
        }
    }
}
