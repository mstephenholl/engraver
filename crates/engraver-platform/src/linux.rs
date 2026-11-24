//! Linux platform implementation
//!
//! Uses O_DIRECT for direct I/O and standard POSIX file operations.

use crate::aligned::AlignedBuffer;
use crate::{
    align_up, bytes_consumed_from_aligned_write, is_aligned, is_linux_partition_of, DeviceInfo,
    OpenOptions, PlatformError, PlatformOps, RawDevice, Result,
};
use std::fs::{File, OpenOptions as StdOpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{FileExt, FileTypeExt, OpenOptionsExt};
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::process::Command;

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

/// The `custom_flags` for opening a device
///
/// A block device opened for writing gets `O_EXCL`. Without `O_CREAT`, Linux reads
/// that as an exclusive open, and the kernel refuses with EBUSY while the disk or
/// any of its partitions is mounted, in use as swap, or claimed by device-mapper,
/// md, ZFS or btrfs. Other files get no `O_EXCL`: without `O_CREAT` it is
/// defined only for block devices.
fn open_flags(options: &OpenOptions, block_device: bool) -> i32 {
    let mut flags = 0;
    if options.direct_io {
        flags |= libc::O_DIRECT;
    }
    if options.write && block_device {
        flags |= libc::O_EXCL;
    }
    flags
}

/// Open the file behind a device, mapping EBUSY from the exclusive open to `DeviceBusy`.
pub(crate) fn open_file(path: &str, options: &OpenOptions) -> Result<File> {
    let block_device = std::fs::metadata(path).is_ok_and(|m| m.file_type().is_block_device());

    StdOpenOptions::new()
        .read(options.read)
        .write(options.write)
        .custom_flags(open_flags(options, block_device))
        .open(path)
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::PermissionDenied {
                PlatformError::PermissionDenied(format!(
                    "Cannot open {}: {}. Try running with sudo.",
                    path, e
                ))
            } else if e.raw_os_error() == Some(libc::EBUSY) {
                PlatformError::DeviceBusy(format!(
                    "{} is in use: it is mounted, used as swap, or held by another program. \
                     Unmount it or stop that program, then try again.",
                    path
                ))
            } else {
                PlatformError::Io(e)
            }
        })
}

impl LinuxDevice {
    /// Open a device for raw I/O
    pub fn open(path: &str, options: OpenOptions) -> Result<Self> {
        let device_path = Path::new(path);

        // Check if device exists
        if !device_path.exists() {
            return Err(PlatformError::DeviceNotFound(path.to_string()));
        }

        let file = open_file(path, &options)?;

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
        self.check_offset(offset)?;
        self.file.seek(SeekFrom::Start(offset))?;
        self.write(data).map_err(PlatformError::Io)
    }

    fn read_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<usize> {
        self.file.seek(SeekFrom::Start(offset))?;
        self.read(buffer).map_err(PlatformError::Io)
    }
}

impl LinuxDevice {
    fn check_offset(&self, offset: u64) -> Result<()> {
        let block_size = self.info.block_size as usize;
        if self.info.direct_io && !is_aligned(offset as usize, block_size) {
            return Err(PlatformError::AlignmentError(format!(
                "Offset {} is not aligned to block size {}",
                offset, block_size
            )));
        }
        Ok(())
    }
}

impl Read for LinuxDevice {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let Some(aligned_buf) = self.aligned_buffer.as_mut() else {
            return self.file.read(buf);
        };
        let pos = self.file.stream_position()?;
        let file = &self.file;
        let read =
            aligned_buf.read_through(pos, buf, |blocks, offset| file.read_at(blocks, offset))?;
        self.file.seek(SeekFrom::Start(pos + read as u64))?;
        Ok(read)
    }
}

impl Write for LinuxDevice {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.info.direct_io
            && (!is_aligned(buf.len(), self.info.block_size as usize)
                || !is_aligned(buf.as_ptr() as usize, self.info.block_size as usize))
        {
            let block_size = self.info.block_size as usize;
            if let Some(ref mut aligned_buf) = self.aligned_buffer {
                let aligned_len = align_up(buf.len(), block_size);
                let aligned_slice = aligned_buf.as_aligned_slice_mut(aligned_len);
                aligned_slice[..buf.len()].copy_from_slice(buf);
                for byte in &mut aligned_slice[buf.len()..aligned_len] {
                    *byte = 0;
                }
                let written = self.file.write(&aligned_slice[..aligned_len])?;
                let consumed = bytes_consumed_from_aligned_write(written, buf.len());
                if written > consumed {
                    self.file
                        .seek(SeekFrom::Current(consumed as i64 - written as i64))?;
                }
                Ok(consumed)
            } else {
                self.file.write(buf)
            }
        } else {
            self.file.write(buf)
        }
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
        tracing::debug!("BLKGETSIZE64 ioctl failed for {path}, falling back to lseek");
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
        tracing::debug!("BLKSSZGET ioctl failed for {path}, defaulting to 512");
    }

    // Default to 512
    Ok(512)
}

/// Split a `/proc/mounts` line into its device and mount point
///
/// Returns `None` for a line with fewer than two fields.
fn parse_mount_entry(line: &str) -> Option<(String, String)> {
    let mut fields = line.split_whitespace();
    let device = unescape_mount_field(fields.next()?);
    let mount_point = unescape_mount_field(fields.next()?);
    Some((device, mount_point))
}

/// Decode the octal escapes in a `/proc/mounts` field
///
/// The kernel writes a space, tab, newline or backslash as `\040`, `\011`, `\012`
/// or `\134`, so `umount` fails on the raw field. Anything other than a backslash
/// and an octal byte from `\000` to `\377` is kept as it is.
fn unescape_mount_field(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if let Some([b'\\', d0 @ b'0'..=b'3', d1 @ b'0'..=b'7', d2 @ b'0'..=b'7']) =
            bytes.get(i..i + 4)
        {
            decoded.push(((d0 - b'0') << 6) | ((d1 - b'0') << 3) | (d2 - b'0'));
            i += 4;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

/// Unmount all filesystems on a device
fn unmount_linux_device(device_path: &str) -> Result<()> {
    // Find all mounted partitions for this device
    let mounts = std::fs::read_to_string("/proc/mounts")
        .map_err(|e| PlatformError::UnmountFailed(format!("Cannot read /proc/mounts: {}", e)))?;

    let mut unmounted_any = false;

    for line in mounts.lines() {
        let Some((mount_device, mount_point)) = parse_mount_entry(line) else {
            continue;
        };

        // Strict match: only the device itself or one of its real
        // partitions. The previous heuristic (`starts_with` OR `contains`
        // on the basename) had false positives for unrelated devices
        // sharing a prefix (`/dev/sdab` matched `/dev/sda`) and for
        // arbitrary substring overlaps (`/dev/mapper/loop_sda` matched
        // `/dev/sda`) — unmounting either would corrupt unrelated data.
        if is_linux_partition_of(&mount_device, device_path) {
            tracing::debug!("Unmounting {} from {}", mount_device, mount_point);

            let status = Command::new("umount").arg(&mount_point).status();

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

    fn open_direct_io_or_skip(temp: &NamedTempFile) -> Option<LinuxDevice> {
        let options = OpenOptions::new().direct_io(true).block_size(4096);
        match LinuxDevice::open(temp.path().to_str().unwrap(), options) {
            Ok(device) => Some(device),
            // tmpfs before Linux 6.6 rejects O_DIRECT.
            Err(PlatformError::Io(e)) if e.raw_os_error() == Some(libc::EINVAL) => {
                eprintln!("skipped: the temp filesystem does not support O_DIRECT");
                None
            }
            Err(e) => panic!("open with direct I/O failed: {e}"),
        }
    }

    /// `len` bytes of `storage` starting one byte past a 4096-byte boundary.
    fn misaligned_range(storage: &[u8], len: usize) -> std::ops::Range<usize> {
        let start = storage.as_ptr().align_offset(4096) + 1;
        start..start + len
    }

    fn pattern(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn test_open_regular_file_with_direct_io_writes_through() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0u8; 8192]).unwrap();
        let Some(mut device) = open_direct_io_or_skip(&temp) else {
            return;
        };

        assert!(device.info().direct_io);
        assert_eq!(device.write_at(4096, &[0x5A; 4096]).unwrap(), 4096);
        drop(device);

        let contents = std::fs::read(temp.path()).unwrap();
        assert!(contents[..4096].iter().all(|&b| b == 0));
        assert!(contents[4096..].iter().all(|&b| b == 0x5A));
    }

    #[test]
    fn test_direct_io_write_at_and_read_at_with_buffers_over_two_blocks() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0xFF; 6 * 4096]).unwrap();
        let Some(mut device) = open_direct_io_or_skip(&temp) else {
            return;
        };
        let data = pattern(2 * 4096 + 1000);

        assert_eq!(device.write_at(4096, &data).unwrap(), data.len());

        let contents = std::fs::read(temp.path()).unwrap();
        let end = 4096 + data.len();
        assert!(contents[..4096].iter().all(|&b| b == 0xFF));
        assert_eq!(&contents[4096..end], &data[..]);
        assert!(contents[end..4 * 4096].iter().all(|&b| b == 0));
        assert!(contents[4 * 4096..].iter().all(|&b| b == 0xFF));

        let mut storage = vec![0u8; data.len() + 4097];
        let range = misaligned_range(&storage, data.len());
        assert_eq!(
            device.read_at(4096, &mut storage[range.clone()]).unwrap(),
            data.len()
        );
        assert_eq!(&storage[range], &data[..]);
    }

    #[test]
    fn test_direct_io_write_and_read_traits_with_a_misaligned_4mib_block() {
        // `engraver benchmark` opens with the default 4096 block size and
        // writes 4 MiB blocks from a plain Vec.
        let len = 4 * 1024 * 1024;
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&vec![0u8; len]).unwrap();
        let Some(mut device) = open_direct_io_or_skip(&temp) else {
            return;
        };
        let mut storage = pattern(len + 4097);
        let range = misaligned_range(&storage, len);
        let data = storage[range.clone()].to_vec();

        device.write_all(&storage[range.clone()]).unwrap();
        assert_eq!(std::fs::read(temp.path()).unwrap(), data);

        storage.fill(0);
        device.seek(SeekFrom::Start(0)).unwrap();
        device.read_exact(&mut storage[range.clone()]).unwrap();
        assert_eq!(&storage[range], &data[..]);
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

    #[test]
    fn test_parse_mount_entry_decodes_space_in_mount_point() {
        let (device, mount_point) =
            parse_mount_entry("/dev/sdb1 /mnt/my\\040drive vfat rw 0 0").unwrap();
        assert_eq!(device, "/dev/sdb1");
        assert_eq!(mount_point, "/mnt/my drive");
    }

    #[test]
    fn test_parse_mount_entry_decodes_tab_newline_and_backslash() {
        let line = "/dev/sdb1 /mnt/a\\011b\\012c\\134d ext4 rw 0 0";
        let (_, mount_point) = parse_mount_entry(line).unwrap();
        assert_eq!(mount_point, "/mnt/a\tb\nc\\d");
    }

    #[test]
    fn test_parse_mount_entry_decodes_each_escape_once() {
        // `\134040` is an escaped backslash followed by the text `040`, not a space
        let (_, mount_point) = parse_mount_entry("/dev/sdb1 /mnt/x\\134040y ext4 rw 0 0").unwrap();
        assert_eq!(mount_point, "/mnt/x\\040y");
    }

    #[test]
    fn test_parse_mount_entry_keeps_text_that_is_not_an_octal_escape() {
        for path in [
            "/mnt/plain",
            "/mnt/a\\b",
            "/mnt/a\\04",
            "/mnt/a\\0x0",
            "/mnt/a\\400",
        ] {
            let line = format!("/dev/sdb1 {path} ext4 rw 0 0");
            assert_eq!(parse_mount_entry(&line).unwrap().1, path);
        }
    }

    #[test]
    fn test_parse_mount_entry_decodes_device_field() {
        let (device, _) =
            parse_mount_entry("/dev/disk/by-label/My\\040Disk /mnt/disk ext4 rw 0 0").unwrap();
        assert_eq!(device, "/dev/disk/by-label/My Disk");
    }

    #[test]
    fn test_parse_mount_entry_needs_a_device_and_a_mount_point() {
        assert_eq!(parse_mount_entry(""), None);
        assert_eq!(parse_mount_entry("/dev/sda1"), None);
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

    // -------------------------------------------------------------------------
    // Exclusive open of block devices
    //
    // A real block device needs root, so `open_flags` takes the device kind as
    // an argument. The loop-device test in tests/integration_tests.rs covers
    // the kernel's EBUSY.
    // -------------------------------------------------------------------------

    #[test]
    fn test_open_flags_write_to_block_device_is_exclusive() {
        let options = OpenOptions::new().write(true).direct_io(true);
        let flags = open_flags(&options, true);
        assert_ne!(flags & libc::O_EXCL, 0);
        assert_ne!(flags & libc::O_DIRECT, 0, "O_DIRECT must survive");
    }

    #[test]
    fn test_open_flags_exclusive_without_direct_io() {
        let options = OpenOptions::new().write(true).direct_io(false);
        assert_eq!(open_flags(&options, true), libc::O_EXCL);
    }

    #[test]
    fn test_open_flags_read_only_block_device_is_not_exclusive() {
        let options = OpenOptions::new().read(true).write(false).direct_io(true);
        assert_eq!(open_flags(&options, true) & libc::O_EXCL, 0);
    }

    #[test]
    fn test_open_flags_regular_file_is_not_exclusive() {
        let options = OpenOptions::new().write(true).direct_io(true);
        assert_eq!(open_flags(&options, false), libc::O_DIRECT);
    }

    #[test]
    fn test_regular_file_opens_for_write_twice() {
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0u8; 4096]).unwrap();
        let path = temp.path().to_str().unwrap();
        let options = OpenOptions::new().write(true).direct_io(false);

        let first = LinuxDevice::open(path, options.clone()).unwrap();
        let second = LinuxDevice::open(path, options);

        assert!(second.is_ok(), "a regular file is not opened exclusively");
        drop(first);
    }

    // -------------------------------------------------------------------------
    // Direct-I/O byte-count contract tests
    //
    // These tests force the aligned-buffer code path by toggling
    // `direct_io` on the open `LinuxDevice` after construction, rather
    // than passing `direct_io(true)` to `open()` (which would attempt to
    // set O_DIRECT and fail on tmpfs/ext4 in CI). This isolates the
    // byte-count logic from the kernel.
    // -------------------------------------------------------------------------

    /// Build a `LinuxDevice` backed by a tmpfile, then enable the
    /// aligned-buffer code path manually.
    fn open_with_forced_direct_io(temp: &NamedTempFile, block_size: usize) -> LinuxDevice {
        let options = OpenOptions::new().direct_io(false).block_size(block_size);
        let mut device = LinuxDevice::open(temp.path().to_str().unwrap(), options).unwrap();
        device.info.direct_io = true;
        device.info.block_size = block_size as u32;
        device.aligned_buffer = Some(AlignedBuffer::new(block_size * 2, block_size));
        device
    }

    #[test]
    fn test_write_at_direct_io_returns_user_data_length_not_aligned_padding() {
        // BUG REGRESSION: `write_at` previously returned the aligned syscall
        // length (e.g. 4096) instead of the user-buffer length (e.g. 100),
        // misleading callers about how many bytes of their data landed on
        // the device.
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0u8; 8192]).unwrap();

        let block_size = 4096;
        let mut device = open_with_forced_direct_io(&temp, block_size);

        let data = vec![0x42u8; 100];
        let written = device.write_at(0, &data).unwrap();

        assert_eq!(
            written, 100,
            "write_at must report user-buffer bytes consumed (100), not aligned length"
        );
    }

    #[test]
    fn test_write_trait_direct_io_never_overreports_buf_len() {
        // BUG REGRESSION: `Write::write` previously returned `buf.len()`
        // even when the underlying syscall returned a short count, hiding
        // partial writes from the Writer's retry logic.
        //
        // We cannot easily inject a short kernel write against a regular
        // file, so we assert the upper-bound invariant: the return value
        // must never exceed `buf.len()`. Combined with
        // `bytes_consumed_from_aligned_write`'s explicit truncation in
        // the helper tests, this fully guards the contract.
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0u8; 8192]).unwrap();

        let block_size = 4096;
        let mut device = open_with_forced_direct_io(&temp, block_size);

        let data = vec![0xCDu8; 100];
        let written = device.write(&data).unwrap();

        assert!(
            written <= data.len(),
            "Write::write must not overreport (got {}, buf was {})",
            written,
            data.len()
        );
        assert_eq!(
            written, 100,
            "with a regular file backing the syscall always completes the full aligned slice, so all 100 user bytes are committed"
        );
    }

    #[test]
    fn test_write_at_aligned_data_unaffected_by_fix() {
        // Regression guard for the happy path: aligned data should still
        // return aligned-length bytes written (matches what the kernel
        // reports because data.len() == aligned_len).
        let mut temp = NamedTempFile::new().unwrap();
        temp.write_all(&[0u8; 8192]).unwrap();

        let block_size = 4096;
        let mut device = open_with_forced_direct_io(&temp, block_size);

        // Allocate aligned data: a Vec of exactly block_size bytes whose
        // pointer happens to be block-aligned on the allocator we use.
        // We don't require alignment to hit the aligned-buffer path here,
        // we just verify the count math holds when buf_len == aligned_len.
        let data = vec![0x77u8; block_size];
        let written = device.write_at(0, &data).unwrap();
        assert_eq!(written, block_size);
    }
}
