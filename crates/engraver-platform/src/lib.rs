//! # Engraver Platform
//!
//! Platform-specific adapters for raw device I/O and system operations.
//!
//! This crate provides low-level access to block devices for writing disk images.
//! It handles platform differences in device access, unmounting, and synchronization.
//!
//! ## Safety
//!
//! This crate performs raw device I/O which can destroy data. All operations
//! require explicit device paths and should only be used after validation
//! by the `engraver-detect` crate.

#![warn(missing_docs)]
#![warn(clippy::all)]

use std::io::{Read, Seek, Write};
use thiserror::Error;

/// Platform-specific errors
#[derive(Error, Debug)]
pub enum PlatformError {
    /// IO operation failed
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Device access denied (need elevated privileges)
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Device is busy or locked
    #[error("Device busy: {0}")]
    DeviceBusy(String),

    /// Device not found
    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    /// Failed to unmount device
    #[error("Unmount failed: {0}")]
    UnmountFailed(String),

    /// Operation not supported on this platform
    #[error("Not supported: {0}")]
    NotSupported(String),

    /// Command execution failed
    #[error("Command failed: {0}")]
    CommandFailed(String),

    /// Alignment error for direct I/O
    #[error("Alignment error: {0}")]
    AlignmentError(String),
}

/// Result type for platform operations
pub type Result<T> = std::result::Result<T, PlatformError>;

/// Options for opening a device
#[derive(Debug, Clone)]
pub struct OpenOptions {
    /// Use direct I/O (bypass page cache)
    pub direct_io: bool,

    /// Open for reading
    pub read: bool,

    /// Open for writing
    pub write: bool,

    /// Block size for alignment (typically 512 or 4096)
    pub block_size: usize,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            direct_io: true,
            read: true,
            write: true,
            block_size: 4096,
        }
    }
}

impl OpenOptions {
    /// Create new options with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Set direct I/O mode
    pub fn direct_io(mut self, direct: bool) -> Self {
        self.direct_io = direct;
        self
    }

    /// Set read access
    pub fn read(mut self, read: bool) -> Self {
        self.read = read;
        self
    }

    /// Set write access
    pub fn write(mut self, write: bool) -> Self {
        self.write = write;
        self
    }

    /// Set block size for alignment
    pub fn block_size(mut self, size: usize) -> Self {
        self.block_size = size;
        self
    }
}

/// Information about an open device
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    /// Device path
    pub path: String,

    /// Total size in bytes
    pub size: u64,

    /// Physical block size
    pub block_size: u32,

    /// Whether direct I/O is enabled
    pub direct_io: bool,
}

/// Trait for raw device I/O operations
pub trait RawDevice: Read + Write + Seek + Send {
    /// Get information about the device
    fn info(&self) -> &DeviceInfo;

    /// Get the device size in bytes
    fn size(&self) -> u64 {
        self.info().size
    }

    /// Sync all pending writes to the device
    fn sync(&self) -> Result<()>;

    /// Write data at a specific offset
    fn write_at(&mut self, offset: u64, data: &[u8]) -> Result<usize>;

    /// Read data from a specific offset
    fn read_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<usize>;
}

/// Platform operations interface
pub trait PlatformOps {
    /// Open a device for raw I/O
    fn open_device(path: &str, options: OpenOptions) -> Result<Box<dyn RawDevice>>;

    /// Unmount all filesystems on a device
    fn unmount_device(path: &str) -> Result<()>;

    /// Sync all pending writes system-wide
    fn sync_all() -> Result<()>;

    /// Check if running with elevated privileges
    fn has_elevated_privileges() -> bool;

    /// Get the recommended block size for a device
    fn get_block_size(path: &str) -> Result<u32>;
}

/// Align a value up to the given alignment
#[inline]
pub fn align_up(value: usize, alignment: usize) -> usize {
    if alignment == 0 {
        return value;
    }
    (value + alignment - 1) & !(alignment - 1)
}

/// Align a value down to the given alignment
#[inline]
pub fn align_down(value: usize, alignment: usize) -> usize {
    if alignment == 0 {
        return value;
    }
    value & !(alignment - 1)
}

/// Check if a value is aligned to the given alignment
// Note: Using manual check instead of `is_multiple_of()` for nightly sanitizer compatibility
#[allow(clippy::manual_is_multiple_of)]
#[inline]
pub fn is_aligned(value: usize, alignment: usize) -> bool {
    if alignment == 0 {
        return true;
    }
    value % alignment == 0
}

/// Check if a pointer is aligned to the given alignment
#[inline]
pub fn is_ptr_aligned<T>(ptr: *const T, alignment: usize) -> bool {
    is_aligned(ptr as usize, alignment)
}

// Platform-specific implementations
cfg_if::cfg_if! {
    if #[cfg(target_os = "linux")] {
        mod linux;
        pub use linux::LinuxPlatform as Platform;
    } else if #[cfg(target_os = "macos")] {
        mod macos;
        pub use macos::MacOSPlatform as Platform;
    } else if #[cfg(target_os = "windows")] {
        mod windows;
        pub use windows::WindowsPlatform as Platform;
    }
}

// Re-export the open function for convenience
cfg_if::cfg_if! {
    if #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))] {
        /// Open a device for raw I/O using platform defaults
        pub fn open_device(path: &str, options: OpenOptions) -> Result<Box<dyn RawDevice>> {
            Platform::open_device(path, options)
        }

        /// Unmount all filesystems on a device
        pub fn unmount_device(path: &str) -> Result<()> {
            Platform::unmount_device(path)
        }

        /// Check if running with elevated privileges
        pub fn has_elevated_privileges() -> bool {
            Platform::has_elevated_privileges()
        }

        /// Sync all pending writes
        pub fn sync_all() -> Result<()> {
            Platform::sync_all()
        }
    } else {
        /// Open a device (unsupported platform)
        pub fn open_device(_path: &str, _options: OpenOptions) -> Result<Box<dyn RawDevice>> {
            Err(PlatformError::NotSupported("Platform not supported".to_string()))
        }

        /// Unmount a device (unsupported platform)
        pub fn unmount_device(_path: &str) -> Result<()> {
            Err(PlatformError::NotSupported("Platform not supported".to_string()))
        }

        /// Check privileges (unsupported platform)
        pub fn has_elevated_privileges() -> bool {
            false
        }

        /// Sync all (unsupported platform)
        pub fn sync_all() -> Result<()> {
            Err(PlatformError::NotSupported("Platform not supported".to_string()))
        }
    }
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Alignment tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_align_up_basic() {
        assert_eq!(align_up(0, 512), 0);
        assert_eq!(align_up(1, 512), 512);
        assert_eq!(align_up(511, 512), 512);
        assert_eq!(align_up(512, 512), 512);
        assert_eq!(align_up(513, 512), 1024);
    }

    #[test]
    fn test_align_up_4k() {
        assert_eq!(align_up(0, 4096), 0);
        assert_eq!(align_up(1, 4096), 4096);
        assert_eq!(align_up(4095, 4096), 4096);
        assert_eq!(align_up(4096, 4096), 4096);
        assert_eq!(align_up(4097, 4096), 8192);
    }

    #[test]
    fn test_align_up_zero_alignment() {
        assert_eq!(align_up(100, 0), 100);
        assert_eq!(align_up(0, 0), 0);
    }

    #[test]
    fn test_align_down_basic() {
        assert_eq!(align_down(0, 512), 0);
        assert_eq!(align_down(1, 512), 0);
        assert_eq!(align_down(511, 512), 0);
        assert_eq!(align_down(512, 512), 512);
        assert_eq!(align_down(513, 512), 512);
        assert_eq!(align_down(1023, 512), 512);
        assert_eq!(align_down(1024, 512), 1024);
    }

    #[test]
    fn test_align_down_4k() {
        assert_eq!(align_down(0, 4096), 0);
        assert_eq!(align_down(4095, 4096), 0);
        assert_eq!(align_down(4096, 4096), 4096);
        assert_eq!(align_down(8191, 4096), 4096);
    }

    #[test]
    fn test_align_down_zero_alignment() {
        assert_eq!(align_down(100, 0), 100);
    }

    #[test]
    fn test_is_aligned() {
        assert!(is_aligned(0, 512));
        assert!(is_aligned(512, 512));
        assert!(is_aligned(1024, 512));
        assert!(!is_aligned(1, 512));
        assert!(!is_aligned(513, 512));

        assert!(is_aligned(0, 4096));
        assert!(is_aligned(4096, 4096));
        assert!(!is_aligned(1, 4096));
    }

    #[test]
    fn test_is_aligned_zero() {
        assert!(is_aligned(0, 0));
        assert!(is_aligned(100, 0));
    }

    // -------------------------------------------------------------------------
    // OpenOptions tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_open_options_default() {
        let opts = OpenOptions::default();
        assert!(opts.direct_io);
        assert!(opts.read);
        assert!(opts.write);
        assert_eq!(opts.block_size, 4096);
    }

    #[test]
    fn test_open_options_builder() {
        let opts = OpenOptions::new()
            .direct_io(false)
            .read(true)
            .write(false)
            .block_size(512);

        assert!(!opts.direct_io);
        assert!(opts.read);
        assert!(!opts.write);
        assert_eq!(opts.block_size, 512);
    }

    // -------------------------------------------------------------------------
    // Error tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_error_display() {
        let err = PlatformError::PermissionDenied("need root".to_string());
        assert!(err.to_string().contains("Permission denied"));
        assert!(err.to_string().contains("need root"));

        let err = PlatformError::DeviceBusy("/dev/sdb".to_string());
        assert!(err.to_string().contains("busy"));

        let err = PlatformError::DeviceNotFound("/dev/sdz".to_string());
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "test");
        let platform_err: PlatformError = io_err.into();
        assert!(matches!(platform_err, PlatformError::Io(_)));
    }

    // -------------------------------------------------------------------------
    // DeviceInfo tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_device_info() {
        let info = DeviceInfo {
            path: "/dev/sdb".to_string(),
            size: 32 * 1024 * 1024 * 1024,
            block_size: 512,
            direct_io: true,
        };

        assert_eq!(info.path, "/dev/sdb");
        assert_eq!(info.size, 32 * 1024 * 1024 * 1024);
        assert_eq!(info.block_size, 512);
        assert!(info.direct_io);
    }
}
