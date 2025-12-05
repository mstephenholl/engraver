//! # Engraver Platform
//!
//! Platform-specific adapters for raw device I/O and system operations.

#![warn(missing_docs)]
#![warn(clippy::all)]

use thiserror::Error;

/// Platform-specific errors
#[derive(Error, Debug)]
pub enum PlatformError {
    /// IO operation failed
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Device access denied
    #[error("Device access denied: {0}")]
    AccessDenied(String),

    /// Device busy
    #[error("Device busy: {0}")]
    DeviceBusy(String),

    /// Operation not supported on this platform
    #[error("Operation not supported: {0}")]
    Unsupported(String),
}

/// Result type for platform operations
pub type Result<T> = std::result::Result<T, PlatformError>;

/// Platform-specific device operations
pub trait DeviceOps {
    /// Open a device for raw read/write access
    fn open_device(path: &str) -> Result<Box<dyn RawDevice>>;

    /// Unmount any filesystems on the device
    fn unmount_device(path: &str) -> Result<()>;

    /// Sync all pending writes to the device
    fn sync_device(path: &str) -> Result<()>;

    /// Check if running with elevated privileges
    fn has_elevated_privileges() -> bool;
}

/// Raw device handle for direct I/O
pub trait RawDevice: Send {
    /// Write a block of data at the specified offset
    fn write_at(&mut self, offset: u64, data: &[u8]) -> Result<usize>;

    /// Read a block of data from the specified offset
    fn read_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<usize>;

    /// Sync all pending writes
    fn sync(&self) -> Result<()>;

    /// Get the device size in bytes
    fn size(&self) -> Result<u64>;
}

cfg_if::cfg_if! {
    if #[cfg(target_os = "linux")] {
        mod linux;
        pub use linux::*;
    } else if #[cfg(target_os = "macos")] {
        mod macos;
        pub use macos::*;
    } else if #[cfg(target_os = "windows")] {
        mod windows;
        pub use windows::*;
    }
}
