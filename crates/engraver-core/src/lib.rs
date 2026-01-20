//! # Engraver Core
//!
//! Core library providing the main functionality for the Engraver disk imaging tool.
//!
//! ## Modules
//!
//! - `source`: Handles local, remote, and compressed image sources
//! - `writer`: High-performance block writing engine with progress tracking
//! - `verifier`: Post-write verification and checksum validation
//! - `error`: Error types and result aliases
//! - `config`: Runtime configuration
//! - `settings`: Persistent user settings from configuration file
//!
//! ## Example
//!
//! ```ignore
//! use engraver_core::{Writer, WriteConfig};
//! use std::fs::File;
//!
//! let source = File::open("image.iso")?;
//! let target = File::create("/dev/sdb")?;
//! let source_size = source.metadata()?.len();
//!
//! let config = WriteConfig::new()
//!     .block_size(4 * 1024 * 1024)
//!     .verify(true);
//!
//! let mut writer = Writer::with_config(config)
//!     .on_progress(|p| println!("{:.1}% - {}", p.percentage(), p.speed_display()));
//!
//! let result = writer.write(source, target, source_size)?;
//! println!("Wrote {} bytes in {:?}", result.bytes_written, result.elapsed);
//! ```

#![warn(missing_docs)]
#![warn(clippy::all)]
#![allow(dead_code)] // Allow during development

pub mod config;
pub mod error;
pub mod resume;
pub mod settings;
pub mod source;
pub mod verifier;
pub mod writer;

pub use config::Config;
pub use error::{Error, Result};
pub use resume::{
    default_checkpoint_dir, validate_checkpoint, CheckpointManager, CheckpointValidation,
    WriteCheckpoint, CHECKPOINT_VERSION,
};
pub use settings::{BehaviorSettings, ChecksumSettings, Settings, SettingsError, WriteSettings};
pub use source::{
    detect_source_type, get_source_size, validate_source, Source, SourceInfo, SourceType,
};
pub use verifier::{
    find_checksum_for_file, parse_checksum_file, verify_write, Checksum, ChecksumAlgorithm,
    ChecksumEntry, VerificationOperation, VerificationProgress, VerificationResult, Verifier,
    VerifyConfig, DEFAULT_VERIFY_BLOCK_SIZE, MAX_VERIFY_BLOCK_SIZE, MIN_VERIFY_BLOCK_SIZE,
};
pub use writer::{
    format_duration, format_speed, WriteConfig, WriteProgress, WriteResult, Writer,
    DEFAULT_BLOCK_SIZE, MAX_BLOCK_SIZE, MIN_BLOCK_SIZE,
};

/// Orchestrates the complete write operation
pub struct Engraver {
    config: Config,
}

impl Engraver {
    /// Create a new Engraver instance with default configuration
    pub fn new() -> Self {
        Self {
            config: Config::default(),
        }
    }

    /// Create a new Engraver instance with custom configuration
    pub fn with_config(config: Config) -> Self {
        Self { config }
    }

    /// Get the current configuration
    pub fn config(&self) -> &Config {
        &self.config
    }
}

impl Default for Engraver {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_engraver_new() {
        let engraver = Engraver::new();
        assert!(engraver.config().verify);
    }

    #[test]
    fn test_engraver_with_config() {
        let config = Config {
            block_size: 1024 * 1024,
            verify: false,
            sync_each_block: true,
            retry_attempts: 5,
        };
        let engraver = Engraver::with_config(config);
        assert!(!engraver.config().verify);
        assert_eq!(engraver.config().block_size, 1024 * 1024);
    }
}
