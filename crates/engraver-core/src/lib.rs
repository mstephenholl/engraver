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
//! - `config`: Configuration and settings

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod config;
pub mod error;
pub mod source;
pub mod verifier;
pub mod writer;

pub use error::{Error, Result};

/// Orchestrates the complete write operation
pub struct Engraver {
    config: config::Config,
}

impl Engraver {
    /// Create a new Engraver instance with default configuration
    pub fn new() -> Self {
        Self {
            config: config::Config::default(),
        }
    }

    /// Create a new Engraver instance with custom configuration
    pub fn with_config(config: config::Config) -> Self {
        Self { config }
    }
}

impl Default for Engraver {
    fn default() -> Self {
        Self::new()
    }
}
