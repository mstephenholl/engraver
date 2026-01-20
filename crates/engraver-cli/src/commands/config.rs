//! Configuration file management command

use anyhow::{Context, Result};
use console::style;
use engraver_core::Settings;

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
}

/// Execute the config command
pub fn execute(args: ConfigArgs) -> Result<()> {
    // Handle --path flag
    if args.path {
        if let Some(path) = Settings::config_path() {
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
        return init_config(args.silent);
    }

    // Default: show current configuration
    show_config(args.json, args.silent)
}

/// Initialize a new configuration file with default values
fn init_config(silent: bool) -> Result<()> {
    let path = Settings::config_path().context("Could not determine configuration directory")?;

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
        .save()
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
fn show_config(json: bool, silent: bool) -> Result<()> {
    if silent {
        return Ok(());
    }

    let settings = Settings::load();
    let config_exists = Settings::config_exists();

    if json {
        // Output as JSON for scripting
        let json_output = serde_json::to_string_pretty(&settings)
            .context("Failed to serialize settings to JSON")?;
        println!("{}", json_output);
    } else {
        // Human-readable output
        println!("{}", style("Engraver Configuration").bold());
        println!();

        if let Some(path) = Settings::config_path() {
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
    use engraver_core::{BehaviorSettings, ChecksumSettings, WriteSettings};
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
            },
            checksum: ChecksumSettings {
                algorithm: "sha512".to_string(),
                auto_detect: true,
            },
            behavior: BehaviorSettings {
                skip_confirmation: false,
                quiet: false,
            },
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
        };
        assert!(!args.init);
        assert!(!args.path);
        assert!(!args.json);
        assert!(!args.silent);
    }

    #[test]
    fn test_show_config_silent() {
        // Silent mode should not panic and return Ok
        let result = show_config(false, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_show_config_json_silent() {
        // Silent mode with JSON should still return Ok
        let result = show_config(true, true);
        assert!(result.is_ok());
    }

    #[test]
    fn test_execute_path_flag() {
        let args = ConfigArgs {
            init: false,
            path: true,
            json: false,
            silent: true,
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
