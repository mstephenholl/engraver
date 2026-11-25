//! Windows platform implementation
//!
//! Uses CreateFile with PhysicalDrive paths.

use crate::{DeviceInfo, OpenOptions, PlatformError, PlatformOps, RawDevice, Result};
use std::io::{Read, Seek, SeekFrom, Write};

#[cfg(target_os = "windows")]
use crate::aligned::AlignedBuffer;
#[cfg(target_os = "windows")]
use std::ptr;
#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_HANDLE_EOF, GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FlushFileBuffers, GetFileSizeEx, ReadFile, WriteFile, FILE_FLAG_NO_BUFFERING,
    FILE_FLAG_WRITE_THROUGH, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
};
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::IO::DeviceIoControl;
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::IO::{OVERLAPPED, OVERLAPPED_0, OVERLAPPED_0_0};

/// Windows platform implementation
pub struct WindowsPlatform;

impl PlatformOps for WindowsPlatform {
    fn open_device(path: &str, options: OpenOptions) -> Result<Box<dyn RawDevice>> {
        #[cfg(target_os = "windows")]
        {
            WindowsDevice::open(path, options).map(|d| Box::new(d) as Box<dyn RawDevice>)
        }
        #[cfg(not(target_os = "windows"))]
        {
            Err(PlatformError::NotSupported(
                "Windows API not available".to_string(),
            ))
        }
    }

    fn unmount_device(path: &str) -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            unmount_windows_device(path)
        }
        #[cfg(not(target_os = "windows"))]
        {
            Err(PlatformError::NotSupported(
                "Windows API not available".to_string(),
            ))
        }
    }

    fn sync_all() -> Result<()> {
        // Windows doesn't have a direct equivalent to sync
        // Flushing happens per-handle
        Ok(())
    }

    fn has_elevated_privileges() -> bool {
        #[cfg(target_os = "windows")]
        {
            is_elevated()
        }
        #[cfg(not(target_os = "windows"))]
        {
            false
        }
    }

    fn get_block_size(_path: &str) -> Result<u32> {
        // Windows typically uses 512 or 4096
        Ok(512)
    }
}

/// Windows device wrapper for raw I/O
#[cfg(target_os = "windows")]
pub struct WindowsDevice {
    handle: HANDLE,
    info: DeviceInfo,
    aligned_buffer: Option<AlignedBuffer>,
    // A FILE_FLAG_NO_BUFFERING handle's own file pointer only moves to
    // sector-aligned positions.
    position: u64,
}

#[cfg(target_os = "windows")]
impl WindowsDevice {
    /// Open a physical drive for raw I/O
    pub fn open(path: &str, options: OpenOptions) -> Result<Self> {
        // Ensure path is in Windows format
        let device_path = normalize_windows_path(path);

        // Convert to wide string
        let wide_path: Vec<u16> = device_path
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

        // Build access flags
        let mut access = 0u32;
        if options.read {
            access |= GENERIC_READ;
        }
        if options.write {
            access |= GENERIC_WRITE;
        }

        // Build flags
        let mut flags = 0u32;
        if options.direct_io {
            flags |= FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH;
        }

        // Open the device
        // SAFETY: CreateFileW is called with a valid null-terminated wide string path.
        // All pointer arguments are valid: wide_path is alive for the call duration,
        // null pointers are used where no value is needed. The returned HANDLE is
        // validated before use (checked against INVALID_HANDLE_VALUE below).
        #[allow(unsafe_code)]
        let handle = unsafe {
            CreateFileW(
                wide_path.as_ptr(),
                access,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                ptr::null(),
                OPEN_EXISTING,
                flags,
                std::ptr::null_mut(), // Template file handle
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            let error = std::io::Error::last_os_error();
            return Err(match error.raw_os_error() {
                Some(5) => PlatformError::PermissionDenied(format!(
                    "Cannot open {}. Run as Administrator.",
                    device_path
                )),
                Some(32) => PlatformError::DeviceBusy(format!(
                    "{} is in use. Close any programs using it.",
                    device_path
                )),
                Some(2) | Some(3) => PlatformError::DeviceNotFound(device_path),
                _ => PlatformError::Io(error),
            });
        }

        // Get device size
        let size = get_device_size(handle, &device_path)?;
        let block_size = options.block_size as u32;

        let info = DeviceInfo {
            path: device_path,
            size,
            block_size,
            direct_io: options.direct_io,
        };

        let aligned_buffer = options
            .direct_io
            .then(|| AlignedBuffer::new(options.block_size * 2, options.block_size));

        Ok(Self {
            handle,
            info,
            aligned_buffer,
            position: 0,
        })
    }
}

// SAFETY: WindowsDevice contains a Windows HANDLE which is safe to send between threads.
// The handle is an opaque pointer managed by the Windows kernel.
#[cfg(target_os = "windows")]
#[allow(unsafe_code)]
unsafe impl Send for WindowsDevice {}

#[cfg(target_os = "windows")]
impl Drop for WindowsDevice {
    fn drop(&mut self) {
        #[allow(unsafe_code)]
        unsafe {
            CloseHandle(self.handle);
        }
    }
}

#[cfg(target_os = "windows")]
impl RawDevice for WindowsDevice {
    fn info(&self) -> &DeviceInfo {
        &self.info
    }

    fn sync(&self) -> Result<()> {
        #[allow(unsafe_code)]
        let result = unsafe { FlushFileBuffers(self.handle) };

        if result == 0 {
            Err(PlatformError::Io(std::io::Error::last_os_error()))
        } else {
            Ok(())
        }
    }

    fn write_at(&mut self, offset: u64, data: &[u8]) -> Result<usize> {
        self.seek(SeekFrom::Start(offset))?;
        self.write(data).map_err(PlatformError::Io)
    }

    fn read_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<usize> {
        self.seek(SeekFrom::Start(offset))?;
        self.read(buffer).map_err(PlatformError::Io)
    }
}

#[cfg(target_os = "windows")]
impl Read for WindowsDevice {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let handle = self.handle;
        let read = match self.aligned_buffer.as_mut() {
            Some(aligned_buf) => {
                aligned_buf.read_through(self.position, buf, |blocks, offset| {
                    read_file_at(handle, blocks, offset)
                })?
            }
            None => read_file_at(handle, buf, self.position)?,
        };
        self.position += read as u64;
        Ok(read)
    }
}

#[cfg(target_os = "windows")]
fn overlapped_at(offset: u64) -> OVERLAPPED {
    OVERLAPPED {
        Internal: 0,
        InternalHigh: 0,
        Anonymous: OVERLAPPED_0 {
            Anonymous: OVERLAPPED_0_0 {
                Offset: offset as u32,
                OffsetHigh: (offset >> 32) as u32,
            },
        },
        hEvent: ptr::null_mut(),
    }
}

#[cfg(target_os = "windows")]
fn read_file_at(handle: HANDLE, buf: &mut [u8], offset: u64) -> std::io::Result<usize> {
    let mut overlapped = overlapped_at(offset);
    let mut bytes_read: u32 = 0;
    // SAFETY: ReadFile is called with a valid HANDLE opened without
    // FILE_FLAG_OVERLAPPED, so it returns only once the read is done.
    // buf.as_mut_ptr() points to buf.len() writable bytes, and bytes_read and
    // overlapped are valid mutable references for the whole call.
    #[allow(unsafe_code)]
    let result = unsafe {
        ReadFile(
            handle,
            buf.as_mut_ptr(),
            buf.len() as u32,
            &mut bytes_read,
            &mut overlapped,
        )
    };

    if result != 0 {
        return Ok(bytes_read as usize);
    }
    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(ERROR_HANDLE_EOF as i32) {
        Ok(0)
    } else {
        Err(error)
    }
}

#[cfg(target_os = "windows")]
fn write_file_at(handle: HANDLE, data: &[u8], offset: u64) -> std::io::Result<usize> {
    let mut overlapped = overlapped_at(offset);
    let mut bytes_written: u32 = 0;
    // SAFETY: WriteFile is called with a valid HANDLE opened without
    // FILE_FLAG_OVERLAPPED, so it returns only once the write is done.
    // data.as_ptr() points to data.len() readable bytes, and bytes_written and
    // overlapped are valid mutable references for the whole call.
    #[allow(unsafe_code)]
    let result = unsafe {
        WriteFile(
            handle,
            data.as_ptr(),
            data.len() as u32,
            &mut bytes_written,
            &mut overlapped,
        )
    };

    if result == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(bytes_written as usize)
    }
}

#[cfg(target_os = "windows")]
impl Write for WindowsDevice {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let written = write_file_at(self.handle, buf, self.position)?;
        self.position += written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        #[allow(unsafe_code)]
        let result = unsafe { FlushFileBuffers(self.handle) };

        if result == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
}

#[cfg(target_os = "windows")]
impl Seek for WindowsDevice {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let (base, delta) = match pos {
            SeekFrom::Start(n) => (n, 0),
            SeekFrom::End(n) => (self.info.size, n),
            SeekFrom::Current(n) => (self.position, n),
        };
        self.position = base.checked_add_signed(delta).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "seek to a negative or overflowing position",
            )
        })?;
        Ok(self.position)
    }
}

/// Normalize a device path for Windows
///
/// Converts various formats to the proper Windows device path:
/// - "1" or "PhysicalDrive1" -> "\\.\PhysicalDrive1"
/// - "\\.\PhysicalDrive1" -> unchanged
fn normalize_windows_path(path: &str) -> String {
    if path.starts_with("\\\\.\\") {
        path.to_string()
    } else if path.starts_with("PhysicalDrive") {
        format!("\\\\.\\{}", path)
    } else if let Ok(n) = path.parse::<u32>() {
        format!("\\\\.\\PhysicalDrive{}", n)
    } else {
        path.to_string()
    }
}

/// Get device size on Windows
#[cfg(target_os = "windows")]
fn get_device_size(handle: HANDLE, _path: &str) -> Result<u64> {
    use windows_sys::Win32::System::Ioctl::{GET_LENGTH_INFORMATION, IOCTL_DISK_GET_LENGTH_INFO};

    #[allow(unsafe_code)]
    let mut length_info: GET_LENGTH_INFORMATION = unsafe { std::mem::zeroed() };
    let mut bytes_returned: u32 = 0;

    #[allow(unsafe_code)]
    let result = unsafe {
        DeviceIoControl(
            handle,
            IOCTL_DISK_GET_LENGTH_INFO,
            ptr::null(),
            0,
            &mut length_info as *mut _ as *mut _,
            std::mem::size_of::<GET_LENGTH_INFORMATION>() as u32,
            &mut bytes_returned,
            ptr::null_mut(),
        )
    };

    if result != 0 {
        Ok(length_info.Length as u64)
    } else {
        // Fallback: try GetFileSizeEx
        let mut size: i64 = 0;
        #[allow(unsafe_code)]
        let result = unsafe { GetFileSizeEx(handle, &mut size) };

        if result != 0 && size > 0 {
            Ok(size as u64)
        } else {
            Err(PlatformError::Io(std::io::Error::last_os_error()))
        }
    }
}

/// Unmount volumes on a Windows physical drive
#[cfg(target_os = "windows")]
fn unmount_windows_device(path: &str) -> Result<()> {
    use std::process::Command;

    let device_path = normalize_windows_path(path);

    // Extract drive number
    let drive_num = device_path
        .trim_start_matches("\\\\.\\PhysicalDrive")
        .parse::<u32>()
        .map_err(|_| PlatformError::DeviceNotFound(path.to_string()))?;

    // Use diskpart or PowerShell to get and unmount volumes
    // This is a simplified approach - a full implementation would enumerate volumes

    let output = Command::new("powershell")
        .args([
            "-Command",
            &format!(
                "Get-Disk -Number {} | Get-Partition | ForEach-Object {{ \
                    if ($_.DriveLetter) {{ \
                        $vol = Get-Volume -DriveLetter $_.DriveLetter; \
                        if ($vol) {{ \
                            Write-Host \"Dismounting $($_.DriveLetter):\"; \
                            Dismount-Volume -DriveLetter $_.DriveLetter -Force \
                        }} \
                    }} \
                }}",
                drive_num
            ),
        ])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            std::thread::sleep(std::time::Duration::from_millis(100));
            Ok(())
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("not found") || stderr.is_empty() {
                // No volumes to unmount
                Ok(())
            } else {
                Err(PlatformError::UnmountFailed(stderr.to_string()))
            }
        }
        Err(e) => Err(PlatformError::CommandFailed(format!(
            "Failed to run PowerShell: {}",
            e
        ))),
    }
}

/// Check if running with elevated privileges (Administrator)
#[cfg(target_os = "windows")]
fn is_elevated() -> bool {
    use windows_sys::Win32::Security::{
        GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    #[allow(unsafe_code)]
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return false;
        }

        let mut elevation: TOKEN_ELEVATION = std::mem::zeroed();
        let mut size: u32 = 0;

        let result = GetTokenInformation(
            token,
            TokenElevation,
            &mut elevation as *mut _ as *mut _,
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut size,
        );

        CloseHandle(token);

        result != 0 && elevation.TokenIsElevated != 0
    }
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // Path normalization tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_normalize_windows_path_number() {
        assert_eq!(normalize_windows_path("0"), "\\\\.\\PhysicalDrive0");
        assert_eq!(normalize_windows_path("1"), "\\\\.\\PhysicalDrive1");
        assert_eq!(normalize_windows_path("10"), "\\\\.\\PhysicalDrive10");
    }

    #[test]
    fn test_normalize_windows_path_name() {
        assert_eq!(
            normalize_windows_path("PhysicalDrive0"),
            "\\\\.\\PhysicalDrive0"
        );
        assert_eq!(
            normalize_windows_path("PhysicalDrive5"),
            "\\\\.\\PhysicalDrive5"
        );
    }

    #[test]
    fn test_normalize_windows_path_full() {
        assert_eq!(
            normalize_windows_path("\\\\.\\PhysicalDrive0"),
            "\\\\.\\PhysicalDrive0"
        );
        assert_eq!(
            normalize_windows_path("\\\\.\\PhysicalDrive1"),
            "\\\\.\\PhysicalDrive1"
        );
    }

    #[test]
    fn test_normalize_windows_path_other() {
        assert_eq!(normalize_windows_path("C:\\test.img"), "C:\\test.img");
        assert_eq!(normalize_windows_path("test.img"), "test.img");
    }

    // -------------------------------------------------------------------------
    // OpenOptions tests (cross-platform)
    // -------------------------------------------------------------------------

    #[test]
    fn test_open_options_for_windows() {
        let opts = crate::OpenOptions::new()
            .direct_io(true)
            .read(true)
            .write(true)
            .block_size(512);

        assert!(opts.direct_io);
        assert!(opts.read);
        assert!(opts.write);
        assert_eq!(opts.block_size, 512);
    }

    // Note: Actual device tests would require Windows and Administrator privileges
    // They are marked as ignored and can be run manually

    #[test]
    #[ignore]
    #[cfg(target_os = "windows")]
    fn test_open_physical_drive() {
        // This test requires Administrator privileges and an actual USB drive
        // Run manually with: cargo test -- --ignored test_open_physical_drive
    }
}

// Stub implementation for non-Windows platforms to allow compilation
#[cfg(not(target_os = "windows"))]
pub struct WindowsDevice {
    info: DeviceInfo,
}

#[cfg(not(target_os = "windows"))]
impl WindowsDevice {
    pub fn open(_path: &str, options: OpenOptions) -> Result<Self> {
        Err(PlatformError::NotSupported(
            "Windows API not available".to_string(),
        ))
    }
}

#[cfg(not(target_os = "windows"))]
impl RawDevice for WindowsDevice {
    fn info(&self) -> &DeviceInfo {
        &self.info
    }

    fn sync(&self) -> Result<()> {
        Err(PlatformError::NotSupported("Not on Windows".to_string()))
    }

    fn write_at(&mut self, _offset: u64, _data: &[u8]) -> Result<usize> {
        Err(PlatformError::NotSupported("Not on Windows".to_string()))
    }

    fn read_at(&mut self, _offset: u64, _buffer: &mut [u8]) -> Result<usize> {
        Err(PlatformError::NotSupported("Not on Windows".to_string()))
    }
}

#[cfg(not(target_os = "windows"))]
impl Read for WindowsDevice {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Not on Windows",
        ))
    }
}

#[cfg(not(target_os = "windows"))]
impl Write for WindowsDevice {
    fn write(&mut self, _buf: &[u8]) -> std::io::Result<usize> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Not on Windows",
        ))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Not on Windows",
        ))
    }
}

#[cfg(not(target_os = "windows"))]
impl Seek for WindowsDevice {
    fn seek(&mut self, _pos: SeekFrom) -> std::io::Result<u64> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Not on Windows",
        ))
    }
}
