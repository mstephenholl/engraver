//! macOS platform implementation
//!
//! Uses raw device nodes (/dev/rdiskN) and diskutil for unmounting.

use crate::{DeviceInfo, OpenOptions, PlatformError, PlatformOps, RawDevice, Result};
use std::fs::{File, OpenOptions as StdOpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::process::Command;

/// macOS platform implementation
pub struct MacOSPlatform;

impl PlatformOps for MacOSPlatform {
    fn open_device(path: &str, options: OpenOptions) -> Result<Box<dyn RawDevice>> {
        MacOSDevice::open(path, options).map(|d| Box::new(d) as Box<dyn RawDevice>)
    }

    fn unmount_device(path: &str) -> Result<()> {
        unmount_macos_device(path)
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
        // Check if running as root
        unsafe { libc::geteuid() == 0 }
    }

    fn get_block_size(path: &str) -> Result<u32> {
        get_device_block_size(path)
    }
}

/// macOS device wrapper for raw I/O
pub struct MacOSDevice {
    file: File,
    info: DeviceInfo,
    #[allow(dead_code)]
    aligned_buffer: Vec<u8>,
}

impl MacOSDevice {
    /// Open a device for raw I/O
    ///
    /// On macOS, we prefer the raw device (/dev/rdiskN) for direct I/O.
    /// If the provided path is /dev/diskN, we automatically convert to /dev/rdiskN.
    pub fn open(path: &str, options: OpenOptions) -> Result<Self> {
        // Convert to raw device path if needed
        let raw_path = to_raw_device_path(path);
        let device_path = Path::new(&raw_path);

        // Check if device exists
        if !device_path.exists() {
            // Try original path
            if !Path::new(path).exists() {
                return Err(PlatformError::DeviceNotFound(path.to_string()));
            }
        }

        let actual_path = if device_path.exists() {
            raw_path.clone()
        } else {
            path.to_string()
        };

        // Build open options
        let mut std_options = StdOpenOptions::new();
        std_options.read(options.read).write(options.write);

        // macOS doesn't have O_DIRECT, but raw devices bypass the buffer cache
        // We use F_NOCACHE via fcntl after opening

        // Try to open the device
        let file = std_options.open(&actual_path).map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                PlatformError::PermissionDenied(format!(
                    "Cannot open {}: {}. Try running with sudo.",
                    actual_path, e
                ))
            } else if e.raw_os_error() == Some(16) {
                // EBUSY
                PlatformError::DeviceBusy(format!(
                    "{} is busy. Try running: diskutil unmountDisk {}",
                    actual_path, path
                ))
            } else {
                PlatformError::Io(e)
            }
        })?;

        // Set F_NOCACHE for direct I/O
        if options.direct_io {
            set_nocache(&file)?;
        }

        // Get device size
        let size = get_device_size(&file, &actual_path)?;
        let block_size = options.block_size as u32;

        let info = DeviceInfo {
            path: actual_path,
            size,
            block_size,
            direct_io: options.direct_io,
        };

        // Create aligned buffer
        let aligned_buffer = vec![0u8; options.block_size * 2];

        Ok(Self {
            file,
            info,
            aligned_buffer,
        })
    }
}

impl RawDevice for MacOSDevice {
    fn info(&self) -> &DeviceInfo {
        &self.info
    }

    fn sync(&self) -> Result<()> {
        let fd = self.file.as_raw_fd();
        let result = unsafe { libc::fsync(fd) };
        if result == 0 {
            Ok(())
        } else {
            Err(PlatformError::Io(std::io::Error::last_os_error()))
        }
    }

    fn write_at(&mut self, offset: u64, data: &[u8]) -> Result<usize> {
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.write(data).map_err(PlatformError::Io)
    }

    fn read_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<usize> {
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read(buffer).map_err(PlatformError::Io)
    }
}

impl Read for MacOSDevice {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(buf)
    }
}

impl Write for MacOSDevice {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.file.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

impl Seek for MacOSDevice {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.file.seek(pos)
    }
}

/// Convert a disk path to its raw device equivalent
///
/// /dev/disk2 -> /dev/rdisk2
/// /dev/disk2s1 -> /dev/rdisk2s1
fn to_raw_device_path(path: &str) -> String {
    if path.starts_with("/dev/disk") && !path.starts_with("/dev/rdisk") {
        path.replacen("/dev/disk", "/dev/rdisk", 1)
    } else {
        path.to_string()
    }
}

/// Set F_NOCACHE on a file descriptor for direct I/O
fn set_nocache(file: &File) -> Result<()> {
    let fd = file.as_raw_fd();

    // F_NOCACHE = 48 on macOS
    const F_NOCACHE: libc::c_int = 48;

    let result = unsafe { libc::fcntl(fd, F_NOCACHE, 1) };

    if result == -1 {
        Err(PlatformError::Io(std::io::Error::last_os_error()))
    } else {
        Ok(())
    }
}

/// Get device size using ioctl
fn get_device_size(file: &File, path: &str) -> Result<u64> {
    let fd = file.as_raw_fd();

    // Try DKIOCGETBLOCKCOUNT * DKIOCGETBLOCKSIZE
    #[cfg(target_os = "macos")]
    {
        // DKIOCGETBLOCKCOUNT = 0x40086419
        // DKIOCGETBLOCKSIZE = 0x40046418
        const DKIOCGETBLOCKCOUNT: libc::c_ulong = 0x40086419;
        const DKIOCGETBLOCKSIZE: libc::c_ulong = 0x40046418;

        let mut block_count: u64 = 0;
        let mut block_size: u32 = 0;

        let result1 = unsafe { libc::ioctl(fd, DKIOCGETBLOCKCOUNT, &mut block_count) };
        let result2 = unsafe { libc::ioctl(fd, DKIOCGETBLOCKSIZE, &mut block_size) };

        if result1 == 0 && result2 == 0 && block_count > 0 && block_size > 0 {
            return Ok(block_count * block_size as u64);
        }
    }

    // Fallback: seek to end
    let _current = file.try_clone()?.stream_position()?;
    let size = file.try_clone()?.seek(SeekFrom::End(0))?;

    if size == 0 {
        Err(PlatformError::Io(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Failed to get size of {}", path),
        )))
    } else {
        Ok(size)
    }
}

/// Get device block size
fn get_device_block_size(path: &str) -> Result<u32> {
    let raw_path = to_raw_device_path(path);
    let file = StdOpenOptions::new()
        .read(true)
        .open(&raw_path)
        .or_else(|_| StdOpenOptions::new().read(true).open(path))
        .map_err(PlatformError::Io)?;

    let fd = file.as_raw_fd();

    #[cfg(target_os = "macos")]
    {
        const DKIOCGETBLOCKSIZE: libc::c_ulong = 0x40046418;

        let mut block_size: u32 = 0;
        let result = unsafe { libc::ioctl(fd, DKIOCGETBLOCKSIZE, &mut block_size) };

        if result == 0 && block_size > 0 {
            return Ok(block_size);
        }
    }

    // Default to 512
    Ok(512)
}

/// Unmount all volumes on a disk using diskutil
fn unmount_macos_device(device_path: &str) -> Result<()> {
    // Extract disk identifier (e.g., "disk2" from "/dev/disk2" or "/dev/rdisk2")
    let disk_id = device_path
        .trim_start_matches("/dev/")
        .trim_start_matches('r');

    tracing::debug!("Unmounting disk: {}", disk_id);

    // Use diskutil unmountDisk to unmount all volumes
    let output = Command::new("diskutil")
        .args(["unmountDisk", &format!("/dev/{}", disk_id)])
        .output()
        .map_err(|e| PlatformError::CommandFailed(format!("Failed to run diskutil: {}", e)))?;

    if output.status.success() {
        // Give the system time to process
        std::thread::sleep(std::time::Duration::from_millis(100));
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);

        // Check if already unmounted or no volumes to unmount
        if stdout.contains("was already unmounted")
            || stdout.contains("Unmount of all volumes")
            || stderr.contains("not currently mounted")
        {
            return Ok(());
        }

        Err(PlatformError::UnmountFailed(format!(
            "diskutil unmountDisk failed: {} {}",
            stdout, stderr
        )))
    }
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
    // Path conversion tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_to_raw_device_path_disk() {
        assert_eq!(to_raw_device_path("/dev/disk2"), "/dev/rdisk2");
        assert_eq!(to_raw_device_path("/dev/disk0"), "/dev/rdisk0");
        assert_eq!(to_raw_device_path("/dev/disk10"), "/dev/rdisk10");
    }

    #[test]
    fn test_to_raw_device_path_partition() {
        assert_eq!(to_raw_device_path("/dev/disk2s1"), "/dev/rdisk2s1");
        assert_eq!(to_raw_device_path("/dev/disk2s2"), "/dev/rdisk2s2");
    }

    #[test]
    fn test_to_raw_device_path_already_raw() {
        assert_eq!(to_raw_device_path("/dev/rdisk2"), "/dev/rdisk2");
        assert_eq!(to_raw_device_path("/dev/rdisk2s1"), "/dev/rdisk2s1");
    }

    #[test]
    fn test_to_raw_device_path_other() {
        assert_eq!(to_raw_device_path("/tmp/test.img"), "/tmp/test.img");
        assert_eq!(to_raw_device_path("test.img"), "test.img");
    }

    // -------------------------------------------------------------------------
    // MacOSDevice tests with temp files
    // -------------------------------------------------------------------------

    #[test]
    fn test_open_nonexistent_device() {
        let result = MacOSDevice::open("/dev/nonexistent_disk_xyz", OpenOptions::default());
        assert!(matches!(result, Err(PlatformError::DeviceNotFound(_))));
    }

    #[test]
    fn test_open_regular_file() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0u8; 4096]).unwrap();

        let options = OpenOptions::new().direct_io(false);
        let result = MacOSDevice::open(temp.path().to_str().unwrap(), options);

        assert!(result.is_ok());
    }

    #[test]
    fn test_device_info() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0u8; 8192]).unwrap();

        let options = OpenOptions::new().direct_io(false);
        let device = MacOSDevice::open(temp.path().to_str().unwrap(), options).unwrap();

        let info = device.info();
        assert_eq!(info.path, temp.path().to_str().unwrap());
    }

    #[test]
    fn test_read_write_regular_file() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0u8; 8192]).unwrap();

        let options = OpenOptions::new().direct_io(false);
        let mut device = MacOSDevice::open(temp.path().to_str().unwrap(), options).unwrap();

        // Write some data
        let data = b"Hello from macOS!";
        let written = device.write_at(0, data).unwrap();
        assert_eq!(written, data.len());

        // Sync
        device.sync().unwrap();

        // Read it back
        let mut buffer = vec![0u8; data.len()];
        let read = device.read_at(0, &mut buffer).unwrap();
        assert_eq!(read, data.len());
        assert_eq!(&buffer, data);
    }

    #[test]
    fn test_write_at_offset() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0u8; 8192]).unwrap();

        let options = OpenOptions::new().direct_io(false);
        let mut device = MacOSDevice::open(temp.path().to_str().unwrap(), options).unwrap();

        // Write at offset
        let data = b"OFFSET";
        device.write_at(1024, data).unwrap();

        // Verify by reading
        let mut buffer = vec![0u8; data.len()];
        device.read_at(1024, &mut buffer).unwrap();
        assert_eq!(&buffer, data);

        // Verify start is still zeros
        let mut start = vec![0u8; 6];
        device.read_at(0, &mut start).unwrap();
        assert_eq!(start, vec![0u8; 6]);
    }

    #[test]
    fn test_sync_regular_file() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0u8; 4096]).unwrap();

        let options = OpenOptions::new().direct_io(false);
        let device = MacOSDevice::open(temp.path().to_str().unwrap(), options).unwrap();

        assert!(device.sync().is_ok());
    }

    // -------------------------------------------------------------------------
    // Platform privilege tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_has_elevated_privileges() {
        // Just verify it doesn't panic
        let _ = MacOSPlatform::has_elevated_privileges();
    }

    // -------------------------------------------------------------------------
    // diskutil command parsing tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_disk_id_extraction() {
        // Test disk identifier extraction logic
        let path = "/dev/disk2";
        let disk_id = path.trim_start_matches("/dev/").trim_start_matches('r');
        assert_eq!(disk_id, "disk2");

        let path = "/dev/rdisk2";
        let disk_id = path.trim_start_matches("/dev/").trim_start_matches('r');
        assert_eq!(disk_id, "disk2");

        let path = "/dev/rdisk2s1";
        let disk_id = path.trim_start_matches("/dev/").trim_start_matches('r');
        assert_eq!(disk_id, "disk2s1");
    }
}
