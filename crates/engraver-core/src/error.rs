//! Error types for the Engraver core library

use thiserror::Error;

/// Main error type for Engraver operations
#[derive(Error, Debug)]
pub enum Error {
    /// Source file not found or inaccessible
    #[error("Source not found: {0}")]
    SourceNotFound(String),

    /// Target device not found or inaccessible
    #[error("Target device not found: {0}")]
    DeviceNotFound(String),

    /// Device is a system drive (safety check failed)
    #[error("Refusing to write to system drive: {0}")]
    SystemDriveProtection(String),

    /// IO error during read/write operations
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Verification failed after write
    #[error("Verification failed at offset {offset}: expected {expected}, got {actual}")]
    VerificationFailed {
        /// Offset where mismatch occurred
        offset: u64,
        /// Expected value
        expected: String,
        /// Actual value
        actual: String,
    },

    /// Network error for remote sources
    #[error("Network error: {0}")]
    Network(String),

    /// Decompression error
    #[error("Decompression error: {0}")]
    Decompression(String),

    /// Permission denied
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Invalid configuration
    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),

    /// Operation was cancelled
    #[error("Operation cancelled")]
    Cancelled,

    /// Partial write occurred
    #[error("Partial write: expected {expected} bytes, wrote {actual} bytes")]
    PartialWrite {
        /// Expected bytes to write
        expected: usize,
        /// Actual bytes written
        actual: usize,
    },

    /// Device is busy
    #[error("Device busy: {0}")]
    DeviceBusy(String),

    /// Size mismatch between source and target
    #[error("Size mismatch: source is {source_size} bytes, target is {target_size} bytes")]
    SizeMismatch {
        /// Source size in bytes
        source_size: u64,
        /// Target size in bytes
        target_size: u64,
    },

    /// Checksum mismatch
    #[error("Checksum mismatch: expected {expected}, got {actual}")]
    ChecksumMismatch {
        /// Expected checksum
        expected: String,
        /// Actual checksum
        actual: String,
    },

    /// Unknown error
    #[error("Unknown error: {0}")]
    Unknown(String),
}

/// Result type alias using the Engraver error type
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let err = Error::SourceNotFound("/path/to/file.iso".to_string());
        assert!(err.to_string().contains("/path/to/file.iso"));

        let err = Error::Cancelled;
        assert_eq!(err.to_string(), "Operation cancelled");

        let err = Error::PartialWrite {
            expected: 4096,
            actual: 2048,
        };
        assert!(err.to_string().contains("4096"));
        assert!(err.to_string().contains("2048"));
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "test");
        let err: Error = io_err.into();
        assert!(matches!(err, Error::Io(_)));
    }

    #[test]
    fn test_verification_failed_error() {
        let err = Error::VerificationFailed {
            offset: 1024,
            expected: "abc123".to_string(),
            actual: "def456".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("1024"));
        assert!(msg.contains("abc123"));
        assert!(msg.contains("def456"));
    }

    #[test]
    fn test_size_mismatch_error() {
        let err = Error::SizeMismatch {
            source_size: 1024,
            target_size: 512,
        };
        let msg = err.to_string();
        assert!(msg.contains("1024"));
        assert!(msg.contains("512"));
    }
}
