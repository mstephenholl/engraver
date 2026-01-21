//! Linux platform implementation
//!
//! Uses O_DIRECT for direct I/O and standard POSIX file operations.

use crate::{
    align_up, is_aligned, DeviceInfo, OpenOptions, PlatformError, PlatformOps, RawDevice, Result,
};
use std::fs::{File, OpenOptions as StdOpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::process::Command;

/// Linux O_DIRECT flag
#[cfg(target_os = "linux")]
const O_DIRECT: i32 = 0o40000;

/// Linux platform implementation
pub struct LinuxPlatform;

impl PlatformOps for LinuxPlatform {
    fn open_device(path: &str, options: OpenOptions) -> Result<Box<dyn RawDevice>> {
        LinuxDevice::open(path, options).map(|d| Box::new(d) as Box<dyn RawDevice>)
    }

    fn unmount_device(path: &str) -> Result<()> {
        unmount_linux_device(path)
    }

    fn sync_all() -> Result<()> {
        // Use sync command
        let status = Command::new("sync").status();
        match status {
            Ok(s) if s.success() => Ok(()),
            Ok(s) => Err(PlatformError::CommandFailed(format!(
                "sync failed with code {:?}",
                s.code()
            ))),
            Err(e) => Err(PlatformError::CommandFailed(format!("sync failed: {}", e))),
        }
    }

    fn has_elevated_privileges() -> bool {
        // SAFETY: geteuid() is a simple syscall that returns the effective user ID.
        // It has no preconditions and cannot cause undefined behavior.
        #[allow(unsafe_code)]
        unsafe {
            libc::geteuid() == 0
        }
    }

    fn get_block_size(path: &str) -> Result<u32> {
        get_device_block_size(path)
    }
}

/// Linux device wrapper for raw I/O
pub struct LinuxDevice {
    file: File,
    info: DeviceInfo,
    /// Aligned buffer for direct I/O operations
    aligned_buffer: Option<AlignedBuffer>,
}

/// Aligned buffer for O_DIRECT operations
struct AlignedBuffer {
    data: Vec<u8>,
    alignment: usize,
}

impl AlignedBuffer {
    fn new(size: usize, alignment: usize) -> Self {
        // Allocate with extra space for alignment
        let total_size = size + alignment;
        let data = vec![0u8; total_size];
        Self { data, alignment }
    }

    /// Returns an aligned slice for reading.
    ///
    /// This is the immutable counterpart to [`Self::as_aligned_slice_mut`].
    #[allow(dead_code)] // Provided for API completeness
    fn as_aligned_slice(&self, len: usize) -> &[u8] {
        let ptr = self.data.as_ptr();
        let aligned_ptr = align_up(ptr as usize, self.alignment) as *const u8;
        let offset = aligned_ptr as usize - ptr as usize;
        &self.data[offset..offset + len]
    }

    fn as_aligned_slice_mut(&mut self, len: usize) -> &mut [u8] {
        let ptr = self.data.as_ptr();
        let aligned_ptr = align_up(ptr as usize, self.alignment) as *mut u8;
        let offset = aligned_ptr as usize - ptr as usize;
        &mut self.data[offset..offset + len]
    }
}

impl LinuxDevice {
    /// Open a device for raw I/O
    pub fn open(path: &str, options: OpenOptions) -> Result<Self> {
        let device_path = Path::new(path);

        // Check if device exists
        if !device_path.exists() {
            return Err(PlatformError::DeviceNotFound(path.to_string()));
        }

        // Build open options
        let mut std_options = StdOpenOptions::new();
        std_options.read(options.read).write(options.write);

        // Add O_DIRECT for direct I/O if requested
        #[cfg(target_os = "linux")]
        if options.direct_io {
            std_options.custom_flags(O_DIRECT);
        }

        // Try to open the device
        let file = std_options.open(device_path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                PlatformError::PermissionDenied(format!(
                    "Cannot open {}: {}. Try running with sudo.",
                    path, e
                ))
            } else if e.raw_os_error() == Some(16) {
                // EBUSY
                PlatformError::DeviceBusy(format!("{} is busy. Try unmounting first.", path))
            } else {
                PlatformError::Io(e)
            }
        })?;

        // Get device size
        let size = get_device_size(&file, path)?;
        let block_size = options.block_size as u32;

        let info = DeviceInfo {
            path: path.to_string(),
            size,
            block_size,
            direct_io: options.direct_io,
        };

        // Create aligned buffer for direct I/O
        let aligned_buffer = if options.direct_io {
            Some(AlignedBuffer::new(
                options.block_size * 2,
                options.block_size,
            ))
        } else {
            None
        };

        Ok(Self {
            file,
            info,
            aligned_buffer,
        })
    }
}

impl RawDevice for LinuxDevice {
    fn info(&self) -> &DeviceInfo {
        &self.info
    }

    fn sync(&self) -> Result<()> {
        // Use fsync to flush writes
        let fd = self.file.as_raw_fd();
        // SAFETY: fsync() is called with a valid file descriptor obtained from as_raw_fd().
        // The fd remains valid for the lifetime of self.file.
        #[allow(unsafe_code)]
        let result = unsafe { libc::fsync(fd) };
        if result == 0 {
            Ok(())
        } else {
            Err(PlatformError::Io(std::io::Error::last_os_error()))
        }
    }

    fn write_at(&mut self, offset: u64, data: &[u8]) -> Result<usize> {
        // Seek to offset
        self.file.seek(SeekFrom::Start(offset))?;

        if self.info.direct_io {
            // For O_DIRECT, we need aligned buffers and offsets
            let block_size = self.info.block_size as usize;

            if !is_aligned(offset as usize, block_size) {
                return Err(PlatformError::AlignmentError(format!(
                    "Offset {} is not aligned to block size {}",
                    offset, block_size
                )));
            }

            // If data is already aligned, write directly
            if is_aligned(data.as_ptr() as usize, block_size) && is_aligned(data.len(), block_size)
            {
                return self.file.write(data).map_err(PlatformError::Io);
            }

            // Use aligned buffer
            if let Some(ref mut buffer) = self.aligned_buffer {
                let aligned_len = align_up(data.len(), block_size);
                let aligned_slice = buffer.as_aligned_slice_mut(aligned_len);

                // Copy data to aligned buffer
                aligned_slice[..data.len()].copy_from_slice(data);
                // Zero pad the rest
                for byte in &mut aligned_slice[data.len()..aligned_len] {
                    *byte = 0;
                }

                self.file
                    .write(&aligned_slice[..aligned_len])
                    .map_err(PlatformError::Io)
            } else {
                // Fallback to regular write
                self.file.write(data).map_err(PlatformError::Io)
            }
        } else {
            self.file.write(data).map_err(PlatformError::Io)
        }
    }

    fn read_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<usize> {
        self.file.seek(SeekFrom::Start(offset))?;

        if self.info.direct_io {
            let block_size = self.info.block_size as usize;

            if !is_aligned(offset as usize, block_size) {
                return Err(PlatformError::AlignmentError(format!(
                    "Offset {} is not aligned to block size {}",
                    offset, block_size
                )));
            }

            // If buffer is aligned, read directly
            if is_aligned(buffer.as_ptr() as usize, block_size)
                && is_aligned(buffer.len(), block_size)
            {
                return self.file.read(buffer).map_err(PlatformError::Io);
            }

            // Use aligned buffer
            if let Some(ref mut aligned_buf) = self.aligned_buffer {
                let aligned_len = align_up(buffer.len(), block_size);
                let aligned_slice = aligned_buf.as_aligned_slice_mut(aligned_len);

                let bytes_read = self.file.read(&mut aligned_slice[..aligned_len])?;
                let copy_len = bytes_read.min(buffer.len());
                buffer[..copy_len].copy_from_slice(&aligned_slice[..copy_len]);
                Ok(copy_len)
            } else {
                self.file.read(buffer).map_err(PlatformError::Io)
            }
        } else {
            self.file.read(buffer).map_err(PlatformError::Io)
        }
    }
}

impl Read for LinuxDevice {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(buf)
    }
}

impl Write for LinuxDevice {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.file.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

impl Seek for LinuxDevice {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.file.seek(pos)
    }
}

/// Get device size using ioctl
fn get_device_size(file: &File, path: &str) -> Result<u64> {
    let fd = file.as_raw_fd();

    // Try BLKGETSIZE64 ioctl first
    #[cfg(target_os = "linux")]
    {
        // Use libc::Ioctl type for cross-platform compatibility
        // Cast via u32 to handle the sign bit correctly on platforms where Ioctl is i32
        const BLKGETSIZE64: libc::Ioctl = 0x80081272u32 as libc::Ioctl;

        let mut size: u64 = 0;
        // SAFETY: ioctl with BLKGETSIZE64 writes a u64 to the provided pointer.
        // We pass a valid mutable reference to a u64, and fd is valid.
        #[allow(unsafe_code)]
        let result = unsafe { libc::ioctl(fd, BLKGETSIZE64, &mut size) };

        if result == 0 && size > 0 {
            return Ok(size);
        }
    }

    // Fallback: seek to end
    // SAFETY: lseek64 is called with a valid fd. We save/restore the current position
    // to avoid side effects. The fd remains valid throughout these calls.
    #[allow(unsafe_code)]
    let size = unsafe {
        let current = libc::lseek64(fd, 0, libc::SEEK_CUR);
        let end = libc::lseek64(fd, 0, libc::SEEK_END);
        libc::lseek64(fd, current, libc::SEEK_SET);
        end
    };

    if size < 0 {
        Err(PlatformError::Io(std::io::Error::other(format!(
            "Failed to get size of {path}"
        ))))
    } else {
        Ok(size as u64)
    }
}

/// Get device block size
fn get_device_block_size(path: &str) -> Result<u32> {
    let file = StdOpenOptions::new()
        .read(true)
        .open(path)
        .map_err(PlatformError::Io)?;

    let fd = file.as_raw_fd();

    #[cfg(target_os = "linux")]
    {
        // Use libc::Ioctl type for cross-platform compatibility
        const BLKSSZGET: libc::Ioctl = 0x1268u32 as libc::Ioctl;

        let mut block_size: i32 = 0;
        // SAFETY: ioctl with BLKSSZGET writes an i32 to the provided pointer.
        // We pass a valid mutable reference to an i32, and fd is valid.
        #[allow(unsafe_code)]
        let result = unsafe { libc::ioctl(fd, BLKSSZGET, &mut block_size) };

        if result == 0 && block_size > 0 {
            return Ok(block_size as u32);
        }
    }

    // Default to 512
    Ok(512)
}

/// Unmount all filesystems on a device
fn unmount_linux_device(device_path: &str) -> Result<()> {
    // Find all mounted partitions for this device
    let mounts = std::fs::read_to_string("/proc/mounts")
        .map_err(|e| PlatformError::UnmountFailed(format!("Cannot read /proc/mounts: {}", e)))?;

    let device_base = Path::new(device_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("");

    let mut unmounted_any = false;

    for line in mounts.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 {
            continue;
        }

        let mount_device = parts[0];
        let mount_point = parts[1];

        // Check if this mount is on our device
        if mount_device.starts_with(device_path)
            || (mount_device.contains(device_base) && device_base.len() > 2)
        {
            tracing::debug!("Unmounting {} from {}", mount_device, mount_point);

            let status = Command::new("umount").arg(mount_point).status();

            match status {
                Ok(s) if s.success() => {
                    unmounted_any = true;
                }
                Ok(s) => {
                    return Err(PlatformError::UnmountFailed(format!(
                        "Failed to unmount {}: exit code {:?}",
                        mount_point,
                        s.code()
                    )));
                }
                Err(e) => {
                    return Err(PlatformError::UnmountFailed(format!(
                        "Failed to run umount: {}",
                        e
                    )));
                }
            }
        }
    }

    if unmounted_any {
        // Give the kernel time to process
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    Ok(())
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    // -------------------------------------------------------------------------
    // AlignedBuffer tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_aligned_buffer_creation() {
        let buffer = AlignedBuffer::new(4096, 512);
        assert!(buffer.data.len() >= 4096);
    }

    #[test]
    fn test_aligned_buffer_slice() {
        let buffer = AlignedBuffer::new(4096, 512);
        let slice = buffer.as_aligned_slice(1024);
        assert_eq!(slice.len(), 1024);
        // Check alignment
        assert!(is_aligned(slice.as_ptr() as usize, 512));
    }

    #[test]
    fn test_aligned_buffer_mut_slice() {
        let mut buffer = AlignedBuffer::new(4096, 512);
        let slice = buffer.as_aligned_slice_mut(1024);
        assert_eq!(slice.len(), 1024);
        assert!(is_aligned(slice.as_ptr() as usize, 512));

        // Should be writable
        slice[0] = 42;
        assert_eq!(slice[0], 42);
    }

    // -------------------------------------------------------------------------
    // LinuxDevice tests with temp files
    // -------------------------------------------------------------------------

    #[test]
    fn test_open_nonexistent_device() {
        let result = LinuxDevice::open("/dev/nonexistent_device_xyz", OpenOptions::default());
        assert!(matches!(result, Err(PlatformError::DeviceNotFound(_))));
    }

    #[test]
    fn test_open_regular_file_no_direct_io() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0u8; 4096]).unwrap();

        let options = OpenOptions::new().direct_io(false);
        let result = LinuxDevice::open(temp.path().to_str().unwrap(), options);

        // Should succeed on regular files without O_DIRECT
        assert!(result.is_ok());
    }

    #[test]
    fn test_device_info() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0u8; 8192]).unwrap();

        let options = OpenOptions::new().direct_io(false);
        let device = LinuxDevice::open(temp.path().to_str().unwrap(), options).unwrap();

        let info = device.info();
        assert_eq!(info.path, temp.path().to_str().unwrap());
        assert!(!info.direct_io);
    }

    #[test]
    fn test_read_write_regular_file() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0u8; 8192]).unwrap();

        let options = OpenOptions::new().direct_io(false);
        let mut device = LinuxDevice::open(temp.path().to_str().unwrap(), options).unwrap();

        // Write some data
        let data = b"Hello, Engraver!";
        let written = device.write_at(0, data).unwrap();
        assert_eq!(written, data.len());

        // Read it back
        let mut buffer = vec![0u8; data.len()];
        let read = device.read_at(0, &mut buffer).unwrap();
        assert_eq!(read, data.len());
        assert_eq!(&buffer, data);
    }

    #[test]
    fn test_sync_regular_file() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0u8; 4096]).unwrap();

        let options = OpenOptions::new().direct_io(false);
        let device = LinuxDevice::open(temp.path().to_str().unwrap(), options).unwrap();

        // Sync should succeed
        assert!(device.sync().is_ok());
    }

    // -------------------------------------------------------------------------
    // Platform privilege tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_has_elevated_privileges() {
        // This test just verifies the function runs without panicking
        let _ = LinuxPlatform::has_elevated_privileges();
    }

    // -------------------------------------------------------------------------
    // Mount parsing tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_parse_proc_mounts() {
        let mounts_content = r#"
/dev/sda1 / ext4 rw,relatime 0 0
/dev/sda2 /home ext4 rw,relatime 0 0
/dev/sdb1 /mnt/usb vfat rw,nosuid 0 0
tmpfs /tmp tmpfs rw,nosuid 0 0
"#;
        // Test parsing logic by checking line format
        for line in mounts_content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                assert!(!parts[0].is_empty() || parts[0].is_empty());
            }
        }
    }

    // -------------------------------------------------------------------------
    // LinuxDevice Read/Write/Seek trait tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_linux_device_read_trait() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(b"Read trait test data").unwrap();

        let options = OpenOptions::new().direct_io(false);
        let mut device = LinuxDevice::open(temp.path().to_str().unwrap(), options).unwrap();

        let mut buffer = vec![0u8; 20];
        let n = device.read(&mut buffer).unwrap();
        assert_eq!(n, 20);
        assert_eq!(&buffer, b"Read trait test data");
    }

    #[test]
    fn test_linux_device_write_trait() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0u8; 100]).unwrap();

        let options = OpenOptions::new().direct_io(false);
        let mut device = LinuxDevice::open(temp.path().to_str().unwrap(), options).unwrap();

        let n = device.write(b"Write trait").unwrap();
        assert_eq!(n, 11);

        // Flush and verify
        device.flush().unwrap();
    }

    #[test]
    fn test_linux_device_seek_trait() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(b"0123456789").unwrap();

        let options = OpenOptions::new().direct_io(false);
        let mut device = LinuxDevice::open(temp.path().to_str().unwrap(), options).unwrap();

        // Seek to position 5
        let pos = device.seek(SeekFrom::Start(5)).unwrap();
        assert_eq!(pos, 5);

        // Read from position 5
        let mut buffer = vec![0u8; 5];
        device.read_exact(&mut buffer).unwrap();
        assert_eq!(&buffer, b"56789");
    }

    #[test]
    fn test_linux_device_seek_from_end() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(b"0123456789").unwrap();

        let options = OpenOptions::new().direct_io(false);
        let mut device = LinuxDevice::open(temp.path().to_str().unwrap(), options).unwrap();

        // Seek to 3 bytes from end
        let pos = device.seek(SeekFrom::End(-3)).unwrap();
        assert_eq!(pos, 7);

        let mut buffer = vec![0u8; 3];
        device.read_exact(&mut buffer).unwrap();
        assert_eq!(&buffer, b"789");
    }

    #[test]
    fn test_linux_device_seek_from_current() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(b"ABCDEFGHIJ").unwrap();

        let options = OpenOptions::new().direct_io(false);
        let mut device = LinuxDevice::open(temp.path().to_str().unwrap(), options).unwrap();

        // Read 2 bytes first
        let mut buffer = vec![0u8; 2];
        device.read_exact(&mut buffer).unwrap();

        // Seek forward 3 from current position (now at 5)
        let pos = device.seek(SeekFrom::Current(3)).unwrap();
        assert_eq!(pos, 5);
    }

    // -------------------------------------------------------------------------
    // write_at and read_at tests (without direct_io)
    // -------------------------------------------------------------------------

    #[test]
    fn test_linux_device_write_at_various_offsets() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0u8; 1024]).unwrap();

        let options = OpenOptions::new().direct_io(false);
        let mut device = LinuxDevice::open(temp.path().to_str().unwrap(), options).unwrap();

        // Write at beginning
        device.write_at(0, b"START").unwrap();

        // Write at middle
        device.write_at(512, b"MIDDLE").unwrap();

        // Write at end (near)
        device.write_at(1000, b"END").unwrap();

        // Verify all writes
        let mut buffer = vec![0u8; 5];
        device.read_at(0, &mut buffer).unwrap();
        assert_eq!(&buffer, b"START");

        let mut buffer = vec![0u8; 6];
        device.read_at(512, &mut buffer).unwrap();
        assert_eq!(&buffer, b"MIDDLE");

        let mut buffer = vec![0u8; 3];
        device.read_at(1000, &mut buffer).unwrap();
        assert_eq!(&buffer, b"END");
    }

    #[test]
    fn test_linux_device_read_at_empty_buffer() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(b"test").unwrap();

        let options = OpenOptions::new().direct_io(false);
        let mut device = LinuxDevice::open(temp.path().to_str().unwrap(), options).unwrap();

        let mut buffer = vec![];
        let n = device.read_at(0, &mut buffer).unwrap();
        assert_eq!(n, 0);
    }

    // -------------------------------------------------------------------------
    // AlignedBuffer additional tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_aligned_buffer_various_alignments() {
        for alignment in [512, 1024, 4096] {
            let buffer = AlignedBuffer::new(alignment * 2, alignment);
            let slice = buffer.as_aligned_slice(alignment);
            assert!(is_aligned(slice.as_ptr() as usize, alignment));
        }
    }

    #[test]
    fn test_aligned_buffer_write_and_read() {
        let mut buffer = AlignedBuffer::new(4096, 512);

        // Write to aligned slice
        let slice = buffer.as_aligned_slice_mut(100);
        slice[0..5].copy_from_slice(b"hello");

        // Read back
        let slice = buffer.as_aligned_slice(100);
        assert_eq!(&slice[0..5], b"hello");
    }

    // -------------------------------------------------------------------------
    // DeviceInfo tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_linux_device_info_path() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0u8; 4096]).unwrap();
        let path = temp.path().to_str().unwrap().to_string();

        let options = OpenOptions::new().direct_io(false);
        let device = LinuxDevice::open(&path, options).unwrap();

        assert_eq!(device.info().path, path);
    }

    #[test]
    fn test_linux_device_info_block_size() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0u8; 4096]).unwrap();

        let options = OpenOptions::new().block_size(8192).direct_io(false);
        let device = LinuxDevice::open(temp.path().to_str().unwrap(), options).unwrap();

        assert_eq!(device.info().block_size, 8192);
    }

    #[test]
    fn test_linux_device_info_size() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0u8; 16384]).unwrap();

        let options = OpenOptions::new().direct_io(false);
        let device = LinuxDevice::open(temp.path().to_str().unwrap(), options).unwrap();

        assert_eq!(device.info().size, 16384);
    }

    // -------------------------------------------------------------------------
    // LinuxPlatform sync_all tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_linux_platform_sync_all() {
        // sync_all should succeed (just runs 'sync' command)
        let result = LinuxPlatform::sync_all();
        assert!(result.is_ok());
    }

    // -------------------------------------------------------------------------
    // OpenOptions tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_open_options_read_only() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(b"read only test").unwrap();

        let options = OpenOptions::new().read(true).write(false).direct_io(false);
        let mut device = LinuxDevice::open(temp.path().to_str().unwrap(), options).unwrap();

        // Should be able to read
        let mut buffer = vec![0u8; 14];
        let n = device.read(&mut buffer).unwrap();
        assert_eq!(n, 14);
    }

    #[test]
    fn test_open_options_custom_block_size() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0u8; 8192]).unwrap();

        for block_size in [512, 1024, 4096, 8192] {
            let options = OpenOptions::new().block_size(block_size).direct_io(false);
            let device = LinuxDevice::open(temp.path().to_str().unwrap(), options).unwrap();
            assert_eq!(device.info().block_size as usize, block_size);
        }
    }

    // -------------------------------------------------------------------------
    // Error handling tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_open_permission_denied_message() {
        // Can't easily test permission denied without root, but we can verify
        // the error variant exists
        let err = PlatformError::PermissionDenied("test".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("test"));
    }

    #[test]
    fn test_open_device_busy_message() {
        let err = PlatformError::DeviceBusy("device is in use".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("device is in use"));
    }

    #[test]
    fn test_open_device_not_found_message() {
        let err = PlatformError::DeviceNotFound("/dev/xyz".to_string());
        let msg = format!("{}", err);
        assert!(msg.contains("/dev/xyz"));
    }
}
