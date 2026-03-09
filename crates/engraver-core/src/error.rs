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

    /// Partition table parsing error
    #[error("Failed to parse partition table: {0}")]
    PartitionParseError(String),
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

    #[test]
    fn test_partition_parse_error() {
        let err = Error::PartitionParseError("Invalid MBR signature".to_string());
        let msg = err.to_string();
        assert!(msg.contains("partition table"));
        assert!(msg.contains("Invalid MBR signature"));
    }

    #[test]
    fn test_partition_parse_error_gpt() {
        let err = Error::PartitionParseError("GPT header checksum mismatch".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Failed to parse partition table"));
        assert!(msg.contains("GPT header checksum mismatch"));
    }

    #[test]
    fn test_partition_parse_error_empty() {
        let err = Error::PartitionParseError(String::new());
        let msg = err.to_string();
        assert!(msg.contains("Failed to parse partition table"));
    }

    #[test]
    fn test_error_display_device_not_found() {
        let err = Error::DeviceNotFound("/dev/sdz".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Target device not found"));
        assert!(msg.contains("/dev/sdz"));
    }

    #[test]
    fn test_error_display_system_drive_protection() {
        let err = Error::SystemDriveProtection("/dev/sda".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Refusing to write to system drive"));
        assert!(msg.contains("/dev/sda"));
    }

    #[test]
    fn test_error_display_network() {
        let err = Error::Network("connection timeout".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Network error"));
        assert!(msg.contains("connection timeout"));
    }

    #[test]
    fn test_error_display_decompression() {
        let err = Error::Decompression("invalid gzip header".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Decompression error"));
        assert!(msg.contains("invalid gzip header"));
    }

    #[test]
    fn test_error_display_permission_denied() {
        let err = Error::PermissionDenied("need sudo".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Permission denied"));
        assert!(msg.contains("need sudo"));
    }

    #[test]
    fn test_error_display_invalid_config() {
        let err = Error::InvalidConfig("block size must be positive".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Invalid configuration"));
    }

    #[test]
    fn test_error_display_device_busy() {
        let err = Error::DeviceBusy("/dev/sdb".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Device busy"));
        assert!(msg.contains("/dev/sdb"));
    }

    #[test]
    fn test_error_display_unknown() {
        let err = Error::Unknown("something unexpected".to_string());
        let msg = err.to_string();
        assert!(msg.contains("Unknown error"));
        assert!(msg.contains("something unexpected"));
    }

    #[test]
    fn test_error_display_checksum_mismatch() {
        let err = Error::ChecksumMismatch {
            expected: "abc123".to_string(),
            actual: "def456".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("Checksum mismatch"));
        assert!(msg.contains("abc123"));
        assert!(msg.contains("def456"));
    }

    #[test]
    fn test_error_debug_format() {
        let err = Error::Cancelled;
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("Cancelled"));

        let err = Error::PartialWrite {
            expected: 100,
            actual: 50,
        };
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("PartialWrite"));
    }
}
