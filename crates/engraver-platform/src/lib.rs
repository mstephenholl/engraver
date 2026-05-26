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
#[non_exhaustive]
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

/// Compute the number of *user-buffer* bytes committed when a write went
/// through a padded, alignment-sized syscall.
///
/// When direct I/O requires aligned lengths, a write of `buf_len` bytes is
/// copied into an aligned scratch buffer, the tail is zero-padded up to
/// `aligned_len >= buf_len`, and the kernel returns `syscall_returned` —
/// the number of bytes of the *scratch buffer* that were committed.
///
/// The trait contract for `Write::write` and `RawDevice::write_at` is to
/// return the number of bytes consumed from the **user** buffer, never
/// counting padding. This helper enforces that:
///
/// - If the kernel reported ≥ `buf_len` bytes written, all user bytes
///   landed on the device and the helper returns `buf_len`.
/// - If the kernel reported a *short* write (< `buf_len`), only the first
///   `syscall_returned` user bytes made it; the helper returns that.
///
/// Returning the raw `syscall_returned` (which may be the aligned length)
/// would over-report progress and mask short writes — the bug this helper
/// exists to prevent.
#[inline]
pub fn bytes_consumed_from_aligned_write(syscall_returned: usize, buf_len: usize) -> usize {
    syscall_returned.min(buf_len)
}

/// Return `true` when `mount_device` is the same block device as
/// `device_path`, or one of its partitions, under Linux block-device
/// naming conventions.
///
/// Matches:
/// - exact equality (`/dev/sda` ↔ `/dev/sda`)
/// - traditional partitions: `device_path` followed by one or more digits
///   (`/dev/sda` → `/dev/sda1`, `/dev/sda10`)
/// - NVMe / mmc / loop-style partitions: `device_path` followed by `p`
///   and digits, used when the device name already ends in a digit
///   (`/dev/nvme0n1` → `/dev/nvme0n1p1`, `/dev/mmcblk0` → `/dev/mmcblk0p2`)
///
/// Rejects unrelated devices that share a prefix (`/dev/sdab` is NOT a
/// partition of `/dev/sda`) and arbitrary substring overlaps
/// (`/dev/mapper/loop_sda` does NOT match `/dev/sda`). This rejection
/// is the safety property — unmounting an unrelated filesystem because
/// the device name happened to be a substring would be catastrophic.
///
/// This is a Linux naming convention but the helper lives in the
/// platform-neutral module so it can be unit-tested on every host.
pub fn is_linux_partition_of(mount_device: &str, device_path: &str) -> bool {
    if device_path.is_empty() {
        return false;
    }
    if mount_device == device_path {
        return true;
    }
    let Some(suffix) = mount_device.strip_prefix(device_path) else {
        return false;
    };
    if suffix.is_empty() {
        return false; // already covered by equality
    }
    let bytes = suffix.as_bytes();

    // Devices whose name ends in a digit (nvme0n1, mmcblk0, loop0) name
    // their partitions with a `p` separator; otherwise the trailing
    // digits would be ambiguous between "part of the device name" and
    // "partition number".
    let needs_p_separator = device_path
        .chars()
        .last()
        .is_some_and(|c| c.is_ascii_digit());

    let digit_start = if needs_p_separator {
        if bytes[0] != b'p' {
            return false;
        }
        1
    } else {
        0
    };

    if digit_start >= bytes.len() {
        return false;
    }
    bytes[digit_start..].iter().all(|b| b.is_ascii_digit())
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

    #[test]
    fn test_error_display_all_variants() {
        let err = PlatformError::UnmountFailed("still in use".to_string());
        assert!(err.to_string().contains("Unmount failed"));
        assert!(err.to_string().contains("still in use"));

        let err = PlatformError::NotSupported("feature X".to_string());
        assert!(err.to_string().contains("Not supported"));

        let err = PlatformError::CommandFailed("exit code 1".to_string());
        assert!(err.to_string().contains("Command failed"));

        let err = PlatformError::AlignmentError("buffer not 4K aligned".to_string());
        assert!(err.to_string().contains("Alignment error"));
        assert!(err.to_string().contains("buffer not 4K aligned"));
    }

    // -------------------------------------------------------------------------
    // is_ptr_aligned tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_is_ptr_aligned_null() {
        let ptr: *const u8 = std::ptr::null();
        assert!(is_ptr_aligned(ptr, 512));
        assert!(is_ptr_aligned(ptr, 4096));
    }

    #[test]
    fn test_is_ptr_aligned_with_buffer() {
        // Allocate aligned memory
        let data = vec![0u8; 8192];
        let ptr = data.as_ptr();
        let addr = ptr as usize;

        // The vec allocation is typically aligned to at least 8 bytes
        assert!(is_ptr_aligned(ptr, 1));

        // Check alignment based on actual pointer address
        if addr.is_multiple_of(4096) {
            assert!(is_ptr_aligned(ptr, 4096));
        } else {
            assert!(!is_ptr_aligned(ptr, 4096));
        }
    }

    // -------------------------------------------------------------------------
    // Alignment round-trip tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_align_up_then_down_is_idempotent_when_aligned() {
        let aligned = align_up(1000, 512); // 1024
        assert_eq!(align_down(aligned, 512), aligned);
        assert!(is_aligned(aligned, 512));
    }

    #[test]
    fn test_align_down_then_up_preserves_aligned() {
        let value = 4096;
        assert_eq!(align_down(value, 4096), value);
        assert_eq!(align_up(value, 4096), value);
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

    #[test]
    fn test_device_info_clone() {
        let info = DeviceInfo {
            path: "/dev/sdb".to_string(),
            size: 1024,
            block_size: 4096,
            direct_io: false,
        };
        let cloned = info.clone();
        assert_eq!(cloned.path, info.path);
        assert_eq!(cloned.size, info.size);
        assert_eq!(cloned.block_size, info.block_size);
        assert_eq!(cloned.direct_io, info.direct_io);
    }

    // -------------------------------------------------------------------------
    // OpenOptions additional tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_open_options_read_only() {
        let opts = OpenOptions::new().read(true).write(false).direct_io(false);

        assert!(opts.read);
        assert!(!opts.write);
        assert!(!opts.direct_io);
    }

    #[test]
    fn test_open_options_with_custom_block_size() {
        let opts = OpenOptions::new().block_size(512);
        assert_eq!(opts.block_size, 512);

        let opts = OpenOptions::new().block_size(4096);
        assert_eq!(opts.block_size, 4096);
    }

    // -------------------------------------------------------------------------
    // bytes_consumed_from_aligned_write tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_bytes_consumed_full_aligned_write_caps_at_buf_len() {
        // Kernel wrote the full padded slice (e.g. 4096) but the user only
        // supplied 100 bytes. The user contract says "report user bytes
        // consumed", which is 100 — NOT the aligned padding length.
        assert_eq!(bytes_consumed_from_aligned_write(4096, 100), 100);
    }

    #[test]
    fn test_bytes_consumed_short_write_within_user_data() {
        // Kernel wrote less than the user buffer length (e.g. flaky USB).
        // The helper must surface the truncation so the writer's retry
        // logic can advance correctly instead of believing the write
        // succeeded.
        assert_eq!(bytes_consumed_from_aligned_write(50, 100), 50);
    }

    #[test]
    fn test_bytes_consumed_short_write_at_user_boundary() {
        // Kernel wrote exactly the user-data length — no padding was
        // needed or it stopped exactly at the boundary. Either way the
        // answer is buf_len.
        assert_eq!(bytes_consumed_from_aligned_write(100, 100), 100);
    }

    #[test]
    fn test_bytes_consumed_zero_write_is_zero() {
        // No bytes committed at all — propagate zero so callers can
        // distinguish "EOF / EOWOULDBLOCK" from "wrote everything".
        assert_eq!(bytes_consumed_from_aligned_write(0, 100), 0);
    }

    #[test]
    fn test_bytes_consumed_zero_user_buffer() {
        // Degenerate but well-defined: empty user buffer means zero
        // bytes consumed regardless of what the syscall reports.
        assert_eq!(bytes_consumed_from_aligned_write(4096, 0), 0);
    }

    // -------------------------------------------------------------------------
    // is_linux_partition_of tests
    //
    // The Linux unmount path used to compute "is this mount on the target
    // device?" with `starts_with(device_path) || mount_device.contains(base)`.
    // Both halves had false positives that could unmount unrelated
    // filesystems. These tests guard the safe replacement.
    // -------------------------------------------------------------------------

    #[test]
    fn test_partition_of_exact_match() {
        assert!(is_linux_partition_of("/dev/sda", "/dev/sda"));
        assert!(is_linux_partition_of("/dev/nvme0n1", "/dev/nvme0n1"));
    }

    #[test]
    fn test_partition_of_scsi_partitions() {
        assert!(is_linux_partition_of("/dev/sda1", "/dev/sda"));
        assert!(is_linux_partition_of("/dev/sda2", "/dev/sda"));
        assert!(is_linux_partition_of("/dev/sda10", "/dev/sda"));
        assert!(is_linux_partition_of("/dev/sda99", "/dev/sda"));
    }

    #[test]
    fn test_partition_of_nvme_partitions() {
        assert!(is_linux_partition_of("/dev/nvme0n1p1", "/dev/nvme0n1"));
        assert!(is_linux_partition_of("/dev/nvme0n1p10", "/dev/nvme0n1"));
        assert!(is_linux_partition_of("/dev/mmcblk0p1", "/dev/mmcblk0"));
        assert!(is_linux_partition_of("/dev/loop0p1", "/dev/loop0"));
    }

    #[test]
    fn test_partition_of_rejects_prefix_devices_regression_sda_sdab() {
        // REGRESSION: `starts_with("/dev/sda")` used to return true for
        // `/dev/sdab`, which would have unmounted an unrelated drive's
        // partitions.
        assert!(!is_linux_partition_of("/dev/sdab", "/dev/sda"));
        assert!(!is_linux_partition_of("/dev/sdab1", "/dev/sda"));
        assert!(!is_linux_partition_of("/dev/sdaa", "/dev/sda"));
    }

    #[test]
    fn test_partition_of_rejects_substring_matches_regression_loop_sda() {
        // REGRESSION: `mount_device.contains("sda")` used to return true
        // for `/dev/mapper/loop_sda`. With this fix, that mount line is
        // ignored — only true partitions of /dev/sda are matched.
        assert!(!is_linux_partition_of("/dev/mapper/loop_sda", "/dev/sda"));
        assert!(!is_linux_partition_of("/dev/mapper/sda1", "/dev/sda"));
        assert!(!is_linux_partition_of("sda1", "/dev/sda"));
    }

    #[test]
    fn test_partition_of_rejects_nvme_without_p_separator() {
        // For names ending in a digit, the partition separator `p` is
        // mandatory. `/dev/nvme0n11` would be ambiguous (is `1` part of
        // the namespace or a partition?), and the kernel convention is
        // that it cannot exist — only `nvme0n1p1` does. Anything else
        // sharing the prefix is a different device or unrelated.
        assert!(!is_linux_partition_of("/dev/nvme0n11", "/dev/nvme0n1"));
        assert!(!is_linux_partition_of("/dev/nvme0n1a", "/dev/nvme0n1"));
        assert!(!is_linux_partition_of("/dev/mmcblk01", "/dev/mmcblk0"));
    }

    #[test]
    fn test_partition_of_rejects_different_device() {
        assert!(!is_linux_partition_of("/dev/sdb", "/dev/sda"));
        assert!(!is_linux_partition_of("/dev/sdb1", "/dev/sda"));
        assert!(!is_linux_partition_of("/dev/nvme0n2", "/dev/nvme0n1"));
        assert!(!is_linux_partition_of("/dev/nvme1n1", "/dev/nvme0n1"));
        assert!(!is_linux_partition_of("/dev/loop1", "/dev/loop0"));
    }

    #[test]
    fn test_partition_of_rejects_non_partition_suffixes() {
        // Trailing non-digit, non-`p` suffix is never a partition.
        assert!(!is_linux_partition_of("/dev/sda_bak", "/dev/sda"));
        assert!(!is_linux_partition_of("/dev/sda-partition", "/dev/sda"));
        // `p` without digits is not a partition either.
        assert!(!is_linux_partition_of("/dev/nvme0n1p", "/dev/nvme0n1"));
        assert!(!is_linux_partition_of("/dev/nvme0n1pa", "/dev/nvme0n1"));
    }

    #[test]
    fn test_partition_of_empty_device_path() {
        // Defensive: empty input must not match anything.
        assert!(!is_linux_partition_of("/dev/sda", ""));
        assert!(!is_linux_partition_of("", ""));
        assert!(!is_linux_partition_of("", "/dev/sda"));
    }
}
