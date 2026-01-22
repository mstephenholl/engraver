//! Persistent user settings for Engraver
//!
//! Settings are stored in a TOML configuration file at:
//! - Linux/macOS: `~/.config/engraver/engraver_config.toml`
//! - Windows: `%APPDATA%\engraver\engraver_config.toml`
//!
//! # Example Configuration
//!
//! ```toml
//! [write]
//! block_size = "4M"
//! verify = true
//!
//! [checksum]
//! algorithm = "sha256"
//! auto_detect = true
//!
//! [behavior]
//! skip_confirmation = false
//!
//! [benchmark]
//! block_size = "4M"
//! test_size = "256M"
//! pattern = "zeros"
//! passes = 1
//! json = false
//! ```

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Configuration file name
const CONFIG_FILE_NAME: &str = "engraver_config.toml";

/// Application name for config directory
const APP_NAME: &str = "engraver";

/// Default block size string
const DEFAULT_BLOCK_SIZE_STR: &str = "4M";

/// User settings loaded from configuration file
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Write operation settings
    pub write: WriteSettings,

    /// Checksum settings
    pub checksum: ChecksumSettings,

    /// Behavior settings
    pub behavior: BehaviorSettings,

    /// Benchmark settings
    pub benchmark: BenchmarkSettings,
}

/// Settings for write operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct WriteSettings {
    /// Default block size (e.g., "4M", "1M", "512K")
    pub block_size: String,

    /// Whether to verify writes by default
    pub verify: bool,

    /// Whether to enable checkpointing by default
    pub checkpoint: bool,
}

/// Settings for checksum operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct ChecksumSettings {
    /// Default checksum algorithm
    pub algorithm: String,

    /// Whether to auto-detect checksum files
    pub auto_detect: bool,
}

/// General behavior settings
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BehaviorSettings {
    /// Whether to skip confirmation prompts by default
    pub skip_confirmation: bool,

    /// Whether to suppress non-error output
    pub quiet: bool,
}

/// Settings for benchmark operations
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct BenchmarkSettings {
    /// Default block size for benchmarks (e.g., "4M", "1M", "64K")
    pub block_size: String,

    /// Default test size (e.g., "256M", "512M", "1G")
    pub test_size: String,

    /// Default data pattern (zeros, random, sequential)
    pub pattern: String,

    /// Default number of benchmark passes
    pub passes: u32,

    /// Output results in JSON format by default
    pub json: bool,
}

impl Default for BenchmarkSettings {
    fn default() -> Self {
        Self {
            block_size: DEFAULT_BLOCK_SIZE_STR.to_string(),
            test_size: "256M".to_string(),
            pattern: "zeros".to_string(),
            passes: 1,
            json: false,
        }
    }
}

impl Default for WriteSettings {
    fn default() -> Self {
        Self {
            block_size: DEFAULT_BLOCK_SIZE_STR.to_string(),
            verify: false,
            checkpoint: false,
        }
    }
}

impl Default for ChecksumSettings {
    fn default() -> Self {
        Self {
            algorithm: "sha256".to_string(),
            auto_detect: false,
        }
    }
}

impl Settings {
    /// Load settings from the configuration file
    ///
    /// Returns default settings if the file doesn't exist or can't be parsed
    pub fn load() -> Self {
        Self::load_from_path(Self::config_path())
    }

    /// Load settings from a specific path
    pub fn load_from_path(path: Option<PathBuf>) -> Self {
        let Some(path) = path else {
            tracing::debug!("No config path available, using defaults");
            return Self::default();
        };

        if !path.exists() {
            tracing::debug!("Config file not found at {:?}, using defaults", path);
            return Self::default();
        }

        match std::fs::read_to_string(&path) {
            Ok(contents) => match toml::from_str(&contents) {
                Ok(settings) => {
                    tracing::debug!("Loaded settings from {:?}", path);
                    settings
                }
                Err(e) => {
                    tracing::warn!("Failed to parse config file {:?}: {}", path, e);
                    Self::default()
                }
            },
            Err(e) => {
                tracing::warn!("Failed to read config file {:?}: {}", path, e);
                Self::default()
            }
        }
    }

    /// Save settings to the configuration file
    pub fn save(&self) -> Result<PathBuf, SettingsError> {
        self.save_to_path(Self::config_path())
    }

    /// Save settings to a specific path
    pub fn save_to_path(&self, path: Option<PathBuf>) -> Result<PathBuf, SettingsError> {
        let path = path.ok_or(SettingsError::NoConfigDir)?;

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| SettingsError::Io {
                path: parent.to_path_buf(),
                source: e,
            })?;
        }

        let contents = toml::to_string_pretty(self).map_err(SettingsError::Serialize)?;

        std::fs::write(&path, contents).map_err(|e| SettingsError::Io {
            path: path.clone(),
            source: e,
        })?;

        tracing::info!("Saved settings to {:?}", path);
        Ok(path)
    }

    /// Get the path to the configuration file
    pub fn config_path() -> Option<PathBuf> {
        dirs_next::config_dir().map(|p| p.join(APP_NAME).join(CONFIG_FILE_NAME))
    }

    /// Get the path to the configuration directory
    pub fn config_dir() -> Option<PathBuf> {
        dirs_next::config_dir().map(|p| p.join(APP_NAME))
    }

    /// Check if a configuration file exists
    pub fn config_exists() -> bool {
        Self::config_path().is_some_and(|p| p.exists())
    }

    /// Generate a default configuration file content as a string
    pub fn default_config_string() -> String {
        let default = Self::default();
        toml::to_string_pretty(&default)
            .unwrap_or_else(|_| String::from("# Failed to generate default config"))
    }
}

/// Errors that can occur when working with settings
#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    /// No configuration directory available
    #[error("Could not determine configuration directory")]
    NoConfigDir,

    /// Failed to read or write config file
    #[error("I/O error for {path}: {source}")]
    Io {
        /// Path that caused the error
        path: PathBuf,
        /// The underlying error
        source: std::io::Error,
    },

    /// Failed to serialize settings
    #[error("Failed to serialize settings: {0}")]
    Serialize(toml::ser::Error),

    /// Failed to deserialize settings
    #[error("Failed to parse settings: {0}")]
    Deserialize(toml::de::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_settings() {
        let settings = Settings::default();
        assert_eq!(settings.write.block_size, "4M");
        assert!(!settings.write.verify);
        assert!(!settings.write.checkpoint);
        assert_eq!(settings.checksum.algorithm, "sha256");
        assert!(!settings.checksum.auto_detect);
        assert!(!settings.behavior.skip_confirmation);
        assert!(!settings.behavior.quiet);
        // Benchmark defaults
        assert_eq!(settings.benchmark.block_size, "4M");
        assert_eq!(settings.benchmark.test_size, "256M");
        assert_eq!(settings.benchmark.pattern, "zeros");
        assert_eq!(settings.benchmark.passes, 1);
        assert!(!settings.benchmark.json);
    }

    #[test]
    fn test_settings_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("engraver_config.toml");

        let settings = Settings {
            write: WriteSettings {
                block_size: "1M".to_string(),
                verify: true,
                checkpoint: true,
            },
            checksum: ChecksumSettings {
                algorithm: "sha512".to_string(),
                auto_detect: true,
            },
            behavior: BehaviorSettings {
                skip_confirmation: true,
                quiet: false,
            },
            benchmark: BenchmarkSettings {
                block_size: "16M".to_string(),
                test_size: "512M".to_string(),
                pattern: "random".to_string(),
                passes: 3,
                json: true,
            },
        };

        // Save
        settings.save_to_path(Some(config_path.clone())).unwrap();
        assert!(config_path.exists());

        // Load
        let loaded = Settings::load_from_path(Some(config_path));
        assert_eq!(settings, loaded);
    }

    #[test]
    fn test_load_nonexistent_file() {
        let settings =
            Settings::load_from_path(Some(PathBuf::from("/nonexistent/engraver_config.toml")));
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn test_load_no_path() {
        let settings = Settings::load_from_path(None);
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn test_partial_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("engraver_config.toml");

        // Write a partial config - only write section
        let partial_config = r#"
[write]
verify = true
"#;
        std::fs::write(&config_path, partial_config).unwrap();

        let settings = Settings::load_from_path(Some(config_path));

        // Specified value should be set
        assert!(settings.write.verify);
        // Unspecified values should use defaults
        assert_eq!(settings.write.block_size, "4M");
        assert_eq!(settings.checksum.algorithm, "sha256");
    }

    #[test]
    fn test_default_config_string() {
        let config_str = Settings::default_config_string();
        assert!(config_str.contains("[write]"));
        assert!(config_str.contains("[checksum]"));
        assert!(config_str.contains("[behavior]"));
        assert!(config_str.contains("[benchmark]"));
        assert!(config_str.contains("block_size"));
        assert!(config_str.contains("algorithm"));
        assert!(config_str.contains("test_size"));
        assert!(config_str.contains("pattern"));
        assert!(config_str.contains("passes"));
    }

    #[test]
    fn test_config_path() {
        // This test just verifies the function doesn't panic
        // The actual path varies by platform
        let path = Settings::config_path();
        if let Some(p) = path {
            assert!(p.to_string_lossy().contains("engraver"));
            assert!(p.to_string_lossy().contains("engraver_config.toml"));
        }
    }

    #[test]
    fn test_config_dir() {
        // Verify config_dir returns the parent of config_path
        let dir = Settings::config_dir();
        if let Some(d) = dir {
            assert!(d.to_string_lossy().contains("engraver"));
            // Should not contain the filename
            assert!(!d.to_string_lossy().contains("engraver_config.toml"));
        }
    }

    #[test]
    fn test_config_exists_runs() {
        // Just verify it doesn't panic - actual result depends on environment
        let _ = Settings::config_exists();
    }

    #[test]
    fn test_load_invalid_toml() {
        let temp_dir = TempDir::new().unwrap();
        let config_path = temp_dir.path().join("engraver_config.toml");

        // Write invalid TOML
        std::fs::write(&config_path, "this is not valid toml {{{{").unwrap();

        // Should return defaults when parsing fails
        let settings = Settings::load_from_path(Some(config_path));
        assert_eq!(settings, Settings::default());
    }

    #[test]
    fn test_save_to_none_path() {
        let settings = Settings::default();
        let result = settings.save_to_path(None);
        assert!(matches!(result, Err(SettingsError::NoConfigDir)));
    }

    #[test]
    fn test_write_settings_default() {
        let write = WriteSettings::default();
        assert_eq!(write.block_size, "4M");
        assert!(!write.verify);
        assert!(!write.checkpoint);
    }

    #[test]
    fn test_checksum_settings_default() {
        let checksum = ChecksumSettings::default();
        assert_eq!(checksum.algorithm, "sha256");
        assert!(!checksum.auto_detect);
    }

    #[test]
    fn test_benchmark_settings_default() {
        let benchmark = BenchmarkSettings::default();
        assert_eq!(benchmark.block_size, "4M");
        assert_eq!(benchmark.test_size, "256M");
        assert_eq!(benchmark.pattern, "zeros");
        assert_eq!(benchmark.passes, 1);
        assert!(!benchmark.json);
    }

    #[test]
    fn test_behavior_settings_default() {
        let behavior = BehaviorSettings::default();
        assert!(!behavior.skip_confirmation);
        assert!(!behavior.quiet);
    }

    #[test]
    fn test_settings_error_display() {
        let err = SettingsError::NoConfigDir;
        assert!(err.to_string().contains("configuration directory"));

        let io_err = SettingsError::Io {
            path: PathBuf::from("/test/path"),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
        };
        assert!(io_err.to_string().contains("/test/path"));
    }
}
