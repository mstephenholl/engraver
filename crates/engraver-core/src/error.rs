//! Error types for the Engraver core library

use thiserror::Error;

/// Main error type for Engraver operations
#[derive(Error, Debug)]
pub enum Error {
    /// Source file not found or inaccessible
    #[error("Source not found: {path}")]
    SourceNotFound { path: String },

    /// Target device not found or inaccessible
    #[error("Target device not found: {device}")]
    DeviceNotFound { device: String },

    /// Device is a system drive (safety check failed)
    #[error("Refusing to write to system drive: {device}")]
    SystemDriveProtection { device: String },

    /// IO error during read/write operations
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Verification failed after write
    #[error("Verification failed: expected {expected}, got {actual}")]
    VerificationFailed { expected: String, actual: String },

    /// Network error for remote sources
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    /// Decompression error
    #[error("Decompression error: {message}")]
    Decompression { message: String },

    /// Permission denied
    #[error("Permission denied: {message}")]
    PermissionDenied { message: String },

    /// Invalid configuration
    #[error("Invalid configuration: {message}")]
    InvalidConfig { message: String },
}

/// Result type alias using the Engraver error type
pub type Result<T> = std::result::Result<T, Error>;
