//! Configuration file management command

use anyhow::{Context, Result};
use console::style;
use engraver_core::Settings;
use std::path::PathBuf;

/// Arguments for the config command
pub struct ConfigArgs {
    /// Initialize a new configuration file with defaults
    pub init: bool,
    /// Show the path to the configuration file
    pub path: bool,
    /// Show configuration in JSON format
    pub json: bool,
    /// Suppress output (for scripting)
    pub silent: bool,
    /// Custom configuration file path (overrides default)
    pub config_file: Option<PathBuf>,
}

/// Execute the config command
pub fn execute(args: ConfigArgs) -> Result<()> {
    // Determine the effective config path
    let config_path = args.config_file.clone().or_else(Settings::config_path);

    // Handle --path flag
    if args.path {
        if let Some(path) = &config_path {
            if !args.silent {
                println!("{}", path.display());
            }
        } else if !args.silent {
            eprintln!("{}", style("Could not determine config path").yellow());
        }
        return Ok(());
    }

    // Handle --init flag
    if args.init {
        return init_config(config_path, args.silent);
    }

    // Default: show current configuration
    show_config(config_path, args.json, args.silent)
}

/// Initialize a new configuration file with default values
fn init_config(config_path: Option<PathBuf>, silent: bool) -> Result<()> {
    let path = config_path.context("Could not determine configuration directory")?;

    if path.exists() {
        if !silent {
            eprintln!(
                "{} Configuration file already exists at: {}",
                style("Warning:").yellow(),
                path.display()
            );
            eprintln!("Use a text editor to modify it, or delete it to re-initialize.");
        }
        return Ok(());
    }

    let settings = Settings::default();
    let saved_path = settings
        .save_to_path(Some(path))
        .context("Failed to save configuration file")?;

    if !silent {
        println!(
            "{} Created configuration file at: {}",
            style("Success:").green(),
            saved_path.display()
        );
        println!();
        println!("You can edit this file to customize default settings.");
        println!("Example settings:");
        println!();
        println!("  [write]");
        println!("  verify = true        # Always verify writes");
        println!("  block_size = \"4M\"    # Default block size");
        println!();
        println!("  [checksum]");
        println!("  auto_detect = true   # Auto-detect checksum files");
        println!();
        println!("  [behavior]");
        println!("  skip_confirmation = false");
    }

    Ok(())
}

/// Show the current configuration
fn show_config(config_path: Option<PathBuf>, json: bool, silent: bool) -> Result<()> {
    if silent {
        return Ok(());
    }

    let config_exists = config_path.as_ref().is_some_and(|p| p.exists());
    let settings = Settings::load_from_path(config_path.clone());

    if json {
        // Output as JSON for scripting
        let json_output = serde_json::to_string_pretty(&settings)
            .context("Failed to serialize settings to JSON")?;
        println!("{}", json_output);
    } else {
        // Human-readable output
        println!("{}", style("Engraver Configuration").bold());
        println!();

        if let Some(path) = &config_path {
            if config_exists {
                println!("  {} {}", style("Config file:").dim(), path.display());
            } else {
                println!(
                    "  {} {} {}",
                    style("Config file:").dim(),
                    path.display(),
                    style("(not found, using defaults)").yellow()
                );
            }
        }
        println!();

        println!("{}", style("[write]").cyan());
        println!("  block_size = \"{}\"", settings.write.block_size);
        println!("  verify = {}", settings.write.verify);
        println!("  checkpoint = {}", settings.write.checkpoint);
        println!();

        println!("{}", style("[checksum]").cyan());
        println!("  algorithm = \"{}\"", settings.checksum.algorithm);
        println!("  auto_detect = {}", settings.checksum.auto_detect);
        println!();

        println!("{}", style("[behavior]").cyan());
        println!(
            "  skip_confirmation = {}",
            settings.behavior.skip_confirmation
        );
        println!("  quiet = {}", settings.behavior.quiet);

        if !config_exists {
            println!();
            println!(
                "{}",
                style("Run 'engraver config --init' to create a configuration file.").dim()
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use engraver_core::{
        BehaviorSettings, BenchmarkSettings, ChecksumSettings, NetworkSettings, WriteSettings,
    };
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Helper to create test settings with a custom path
    fn create_test_settings(dir: &TempDir) -> (Settings, PathBuf) {
        let config_path = dir.path().join("engraver_config.toml");
        let settings = Settings {
            write: WriteSettings {
                block_size: "2M".to_string(),
                verify: true,
                checkpoint: true,
                retry_attempts: 3,
                retry_delay_ms: 100,
                read_buffer_size: "64K".to_string(),
            },
            checksum: ChecksumSettings {
                algorithm: "sha512".to_string(),
                auto_detect: true,
            },
            behavior: BehaviorSettings {
                skip_confirmation: false,
                quiet: false,
            },
            benchmark: BenchmarkSettings::default(),
            network: NetworkSettings::default(),
        };
        (settings, config_path)
    }

    #[test]
    fn test_config_args_default() {
        let args = ConfigArgs {
            init: false,
            path: false,
            json: false,
            silent: false,
            config_file: None,
        };
        assert!(!args.init);
        assert!(!args.path);
        assert!(!args.json);
        assert!(!args.silent);
        assert!(args.config_file.is_none());
    }

    #[test]
    fn test_show_config_silent() {
        // Silent mode should not panic and return Ok
        let result = show_config(None, false, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_show_config_json_silent() {
        // Silent mode with JSON should still return Ok
        let result = show_config(None, true, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_path_flag() {
        let args = ConfigArgs {
            init: false,
            path: true,
            json: false,
            silent: true,
            config_file: None,
        };
        let result = execute(args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_silent_mode() {
        let args = ConfigArgs {
            init: false,
            path: false,
            json: false,
            silent: true,
            config_file: None,
        };
        let result = execute(args);
        assert!(result.is_ok());
    }

    #[test]
    fn test_settings_save_load_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let (settings, config_path) = create_test_settings(&temp_dir);

        // Save
        settings.save_to_path(Some(config_path.clone())).unwrap();
        assert!(config_path.exists());

        // Load and verify
        let loaded = Settings::load_from_path(Some(config_path));
        assert_eq!(loaded.write.block_size, "2M");
        assert!(loaded.write.verify);
        assert!(loaded.checksum.auto_detect);
        assert_eq!(loaded.checksum.algorithm, "sha512");
    }

    #[test]
    fn test_settings_json_serialization() {
        let settings = Settings::default();
        let json = serde_json::to_string_pretty(&settings);
        assert!(json.is_ok());
        let json_str = json.unwrap();
        assert!(json_str.contains("block_size"));
        assert!(json_str.contains("algorithm"));
        assert!(json_str.contains("skip_confirmation"));
    }
}
