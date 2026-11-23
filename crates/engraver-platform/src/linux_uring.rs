use crate::linux::open_file;
use crate::{
    align_up, bytes_consumed_from_aligned_write, is_aligned, DeviceInfo, OpenOptions,
    PlatformError, Result,
};
use io_uring::{opcode, types, IoUring};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::io::AsRawFd;
use std::path::Path;

/// Also caps writes in flight, so the completion queue (twice this size) cannot overflow.
const RING_SIZE: u32 = 64;

/// io_uring-based device for high-performance writes
pub struct IoUringDevice {
    file: File,
    ring: IoUring,
    info: DeviceInfo,
    aligned_buffer: AlignedBuffer,
}

struct AlignedBuffer {
    data: Vec<u8>,
    alignment: usize,
}

impl AlignedBuffer {
    fn new(alignment: usize) -> Self {
        Self {
            data: Vec::new(),
            alignment,
        }
    }

    fn padded_copy(&mut self, data: &[u8]) -> &[u8] {
        let len = align_up(data.len(), self.alignment);
        if self.data.len() < len + self.alignment {
            self.data = vec![0; len + self.alignment];
        }
        let base = self.data.as_ptr() as usize;
        let start = align_up(base, self.alignment) - base;
        let padded = &mut self.data[start..start + len];
        padded[..data.len()].copy_from_slice(data);
        padded[data.len()..].fill(0);
        padded
    }
}

/// Result of a batch write operation
#[derive(Debug, Clone)]
pub struct BatchWriteResult {
    /// Total bytes written across all operations
    pub bytes_written: u64,
    /// Number of operations completed
    pub operations_completed: usize,
    /// Always 0: any failed write makes `write_batch` return an error instead
    pub operations_failed: usize,
}

impl IoUringDevice {
    /// Open a device with io_uring support
    ///
    /// Returns [`PlatformError::IoUringError`] if io_uring is not available on this system.
    pub fn open(path: &str, options: OpenOptions) -> Result<Self> {
        let device_path = Path::new(path);

        if !device_path.exists() {
            return Err(PlatformError::DeviceNotFound(path.to_string()));
        }

        if options.direct_io && !options.block_size.is_power_of_two() {
            return Err(PlatformError::AlignmentError(format!(
                "Block size {} is not a power of two",
                options.block_size
            )));
        }

        let ring = IoUring::new(RING_SIZE).map_err(|e| {
            PlatformError::IoUringError(format!("Failed to create io_uring: {}", e))
        })?;

        if !supports_write(&ring) {
            return Err(PlatformError::IoUringError(
                "io_uring write operations not supported on this kernel".to_string(),
            ));
        }

        let file = open_file(path, &options)?;

        let size = get_device_size(&file, path)?;

        let info = DeviceInfo {
            path: path.to_string(),
            size,
            block_size: options.block_size as u32,
            direct_io: options.direct_io,
        };

        Ok(Self {
            file,
            ring,
            info,
            aligned_buffer: AlignedBuffer::new(options.block_size),
        })
    }

    /// Get device info
    pub fn info(&self) -> &DeviceInfo {
        &self.info
    }

    /// Write data at a specific offset using io_uring, blocking until it completes
    ///
    /// Returns the number of bytes of `data` written. Under direct I/O, `data`
    /// is copied into an aligned buffer and zero-padded to whole blocks, and the
    /// padding is written too. For best performance, use `write_batch` to submit
    /// multiple writes at once.
    pub fn write_at(&mut self, offset: u64, data: &[u8]) -> Result<usize> {
        let block_size = self.info.block_size as usize;

        if self.info.direct_io && !is_aligned(offset as usize, block_size) {
            return Err(PlatformError::AlignmentError(format!(
                "Offset {} is not aligned to block size {}",
                offset, block_size
            )));
        }

        let buf = if self.info.direct_io {
            self.aligned_buffer.padded_copy(data)
        } else {
            data
        };

        let written = submit_all(&mut self.ring, &self.file, &[(offset, buf)])
            .remove(0)
            .map_err(PlatformError::Io)?;
        Ok(bytes_consumed_from_aligned_write(written, data.len()))
    }

    /// Batch write multiple blocks using io_uring
    ///
    /// Keeps up to 64 writes in flight and returns once all of them have
    /// completed. Under direct I/O the buffers go to the kernel as they are,
    /// so each must already meet O_DIRECT alignment, and every offset is
    /// checked before any write is queued, so a rejected batch writes nothing.
    ///
    /// # Errors
    /// Fails if any write fails or writes fewer bytes than asked; the other
    /// writes still complete.
    pub fn write_batch<'a, I>(&mut self, writes: I) -> Result<BatchWriteResult>
    where
        I: IntoIterator<Item = (u64, &'a [u8])>,
    {
        let block_size = self.info.block_size as usize;
        let writes: Vec<_> = writes.into_iter().collect();

        if self.info.direct_io {
            if let Some((offset, _)) = writes
                .iter()
                .find(|(offset, _)| !is_aligned(*offset as usize, block_size))
            {
                return Err(PlatformError::AlignmentError(format!(
                    "Offset {} is not aligned to block size {}",
                    offset, block_size
                )));
            }
        }

        let results = submit_all(&mut self.ring, &self.file, &writes);

        let mut bytes_written = 0u64;
        let mut failed = 0;
        for (index, (result, (_, data))) in results.into_iter().zip(&writes).enumerate() {
            match result {
                Err(err) => {
                    failed += 1;
                    tracing::warn!("io_uring write {} failed: {}", index, err);
                }
                Ok(written) => {
                    bytes_written += written as u64;
                    if written < data.len() {
                        failed += 1;
                        tracing::warn!(
                            "io_uring write {} was short: {} of {} bytes",
                            index,
                            written,
                            data.len()
                        );
                    }
                }
            }
        }

        if failed > 0 {
            return Err(PlatformError::IoUringError(format!(
                "{} of {} write operations failed",
                failed,
                writes.len()
            )));
        }

        Ok(BatchWriteResult {
            bytes_written,
            operations_completed: writes.len(),
            operations_failed: 0,
        })
    }

    /// Sync all pending writes to disk
    pub fn sync(&self) -> Result<()> {
        self.file.sync_all().map_err(PlatformError::Io)
    }
}

/// Aborts if a panic unwinds out of submit_all while the kernel may still hold pointers into its buffers.
struct AbortOnUnwind;

impl Drop for AbortOnUnwind {
    fn drop(&mut self) {
        if std::thread::panicking() {
            std::process::abort();
        }
    }
}

fn submit_all(
    ring: &mut IoUring,
    file: &File,
    writes: &[(u64, &[u8])],
) -> Vec<std::io::Result<usize>> {
    let _abort_on_unwind = AbortOnUnwind;
    let fd = types::Fd(file.as_raw_fd());
    let mut results = vec![0; writes.len()];
    let mut next = 0;
    let mut unreaped = 0;

    while next < writes.len() || unreaped > 0 {
        {
            let mut sq = ring.submission();
            while next < writes.len() && unreaped < RING_SIZE as usize {
                let (offset, buf) = writes[next];
                let len = u32::try_from(buf.len()).unwrap_or(u32::MAX);
                let entry = opcode::Write::new(fd, buf.as_ptr(), len)
                    .offset(offset)
                    .build()
                    .user_data(next as u64);
                // SAFETY: `buf` stays borrowed until this function returns, and
                // it neither returns nor unwinds before reaping this entry's completion.
                #[allow(unsafe_code)]
                let pushed = unsafe { sq.push(&entry) };
                if pushed.is_err() {
                    break;
                }
                next += 1;
                unreaped += 1;
            }
        }

        match ring.submit_and_wait(1) {
            Ok(_) => {}
            // Interrupted, or short of kernel resources: reap what is there and retry.
            Err(err)
                if matches!(
                    err.raw_os_error(),
                    Some(libc::EINTR | libc::EAGAIN | libc::EBUSY)
                ) => {}
            Err(err) => {
                // The ring itself is broken, and the kernel may still be reading
                // `writes`. Returning or unwinding would let the caller free those
                // buffers mid-write, so stop the process instead.
                let _ = writeln!(
                    std::io::stderr(),
                    "io_uring_enter failed with {} writes unreaped: {}",
                    unreaped,
                    err
                );
                std::process::abort();
            }
        }

        for cqe in ring.completion() {
            if let Some(result) = results.get_mut(cqe.user_data() as usize) {
                *result = cqe.result();
            }
            unreaped -= 1;
        }
    }

    results
        .into_iter()
        .map(|r| {
            if r < 0 {
                Err(std::io::Error::from_raw_os_error(-r))
            } else {
                Ok(r as usize)
            }
        })
        .collect()
}

fn supports_write(ring: &IoUring) -> bool {
    let mut probe = io_uring::Probe::new();
    ring.submitter().register_probe(&mut probe).is_ok() && probe.is_supported(opcode::Write::CODE)
}

fn get_device_size(file: &File, path: &str) -> Result<u64> {
    let fd = file.as_raw_fd();

    // Cast via u32 to handle the sign bit correctly on platforms where Ioctl is i32
    const BLKGETSIZE64: libc::Ioctl = 0x80081272u32 as libc::Ioctl;

    let mut size: u64 = 0;
    // SAFETY: ioctl with BLKGETSIZE64 writes one u64 to the provided pointer,
    // which points at `size`. `fd` stays open because `file` is borrowed.
    #[allow(unsafe_code)]
    let result = unsafe { libc::ioctl(fd, BLKGETSIZE64, &mut size) };

    if result == 0 && size > 0 {
        return Ok(size);
    }
    tracing::debug!("BLKGETSIZE64 ioctl failed for {path}, falling back to seek");

    let mut file = file;
    let current = file.stream_position()?;
    let end = file.seek(SeekFrom::End(0))?;
    file.seek(SeekFrom::Start(current))?;
    Ok(end)
}

impl Read for IoUringDevice {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.file.read(buf)
    }
}

impl Write for IoUringDevice {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let pos = self.file.stream_position()?;
        let written = self.write_at(pos, buf).map_err(std::io::Error::other)?;
        self.file.seek(SeekFrom::Start(pos + written as u64))?;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.sync().map_err(std::io::Error::other)
    }
}

impl Seek for IoUringDevice {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        self.file.seek(pos)
    }
}

/// Check if io_uring is available on this system
pub fn is_io_uring_available() -> bool {
    IoUring::new(8).is_ok_and(|ring| supports_write(&ring))
}

fn kernel_version() -> Option<(u32, u32, u32)> {
    let uname = std::process::Command::new("uname")
        .arg("-r")
        .output()
        .ok()?;
    parse_kernel_version(&String::from_utf8_lossy(&uname.stdout))
}

fn parse_kernel_version(release: &str) -> Option<(u32, u32, u32)> {
    let parts: Vec<&str> = release.trim().split('.').collect();

    if parts.len() >= 2 {
        let major = parts[0].parse().ok()?;
        let minor = parts[1]
            .split(|c: char| !c.is_ascii_digit())
            .next()?
            .parse()
            .ok()?;
        let patch = parts
            .get(2)
            .and_then(|p| p.split(|c: char| !c.is_ascii_digit()).next())
            .and_then(|p| p.parse().ok())
            .unwrap_or(0);
        Some((major, minor, patch))
    } else {
        None
    }
}

/// Check if the kernel version is sufficient for io_uring (5.6+)
pub fn is_kernel_supported() -> bool {
    match kernel_version() {
        Some((major, minor, _)) => major > 5 || (major == 5 && minor >= 6),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    const BLOCK: usize = 4096;

    fn open_temp_or_skip(
        len: usize,
        fill: u8,
        options: OpenOptions,
    ) -> Option<(NamedTempFile, IoUringDevice)> {
        let test = std::thread::current()
            .name()
            .unwrap_or("io_uring test")
            .to_string();
        if !is_io_uring_available() {
            eprintln!("skipped {test}: io_uring is not available on this host");
            return None;
        }
        let temp = NamedTempFile::new().unwrap();
        std::fs::write(temp.path(), vec![fill; len]).unwrap();
        match IoUringDevice::open(temp.path().to_str().unwrap(), options) {
            Ok(device) => Some((temp, device)),
            // tmpfs before Linux 6.6 rejects O_DIRECT.
            Err(PlatformError::Io(err)) if err.raw_os_error() == Some(libc::EINVAL) => {
                eprintln!("skipped {test}: the temp filesystem does not support O_DIRECT");
                None
            }
            Err(err) => panic!("open failed: {err}"),
        }
    }

    fn buffered() -> OpenOptions {
        OpenOptions::new().direct_io(false)
    }

    fn direct() -> OpenOptions {
        OpenOptions::new().direct_io(true).block_size(BLOCK)
    }

    fn aligned(storage: &[u8], len: usize) -> &[u8] {
        let start = storage.as_ptr().align_offset(BLOCK);
        &storage[start..start + len]
    }

    fn pattern(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn write_at_writes_bytes_at_offset() {
        let Some((temp, mut device)) = open_temp_or_skip(8192, 0, buffered()) else {
            return;
        };

        assert_eq!(device.write_at(1000, b"engraver").unwrap(), 8);

        let contents = std::fs::read(temp.path()).unwrap();
        assert_eq!(&contents[1000..1008], b"engraver");
        assert!(contents[..1000].iter().all(|&b| b == 0));
        assert!(contents[1008..].iter().all(|&b| b == 0));
    }

    #[test]
    fn write_at_direct_io_pads_to_whole_blocks() {
        let Some((temp, mut device)) = open_temp_or_skip(6 * BLOCK, 0xFF, direct()) else {
            return;
        };
        let data = pattern(2 * BLOCK + 1000);

        assert_eq!(device.write_at(BLOCK as u64, &data).unwrap(), data.len());

        let contents = std::fs::read(temp.path()).unwrap();
        let end = BLOCK + data.len();
        assert!(contents[..BLOCK].iter().all(|&b| b == 0xFF));
        assert_eq!(&contents[BLOCK..end], &data[..]);
        assert!(contents[end..4 * BLOCK].iter().all(|&b| b == 0));
        assert!(contents[4 * BLOCK..].iter().all(|&b| b == 0xFF));
    }

    #[test]
    fn write_at_rejects_unaligned_offset_under_direct_io() {
        let Some((temp, mut device)) = open_temp_or_skip(2 * BLOCK, 0xFF, direct()) else {
            return;
        };

        let err = device.write_at(512, &[0u8; BLOCK]).unwrap_err();

        assert!(matches!(err, PlatformError::AlignmentError(_)));
        assert!(std::fs::read(temp.path())
            .unwrap()
            .iter()
            .all(|&b| b == 0xFF));
    }

    #[test]
    fn write_at_on_read_only_device_returns_io_error() {
        let Some((_temp, mut device)) = open_temp_or_skip(BLOCK, 0, buffered().write(false)) else {
            return;
        };

        let err = device.write_at(0, b"denied").unwrap_err();

        assert!(matches!(&err, PlatformError::Io(e) if e.raw_os_error() == Some(libc::EBADF)));
    }

    #[test]
    fn write_batch_writes_every_offset() {
        let Some((temp, mut device)) = open_temp_or_skip(4 * BLOCK, 0, buffered()) else {
            return;
        };
        let writes: [(u64, &[u8]); 4] = [
            (3 * BLOCK as u64, b"fourth"),
            (0, b"first"),
            (2 * BLOCK as u64 + 7, b"third"),
            (BLOCK as u64, b"second"),
        ];

        let result = device.write_batch(writes).unwrap();

        assert_eq!(result.bytes_written, 22);
        assert_eq!(result.operations_completed, 4);
        assert_eq!(result.operations_failed, 0);
        let contents = std::fs::read(temp.path()).unwrap();
        for (offset, data) in writes {
            let offset = offset as usize;
            assert_eq!(&contents[offset..offset + data.len()], data);
        }
    }

    #[test]
    fn write_batch_larger_than_the_ring_writes_everything() {
        let count = 5 * RING_SIZE as usize;
        let Some((temp, mut device)) = open_temp_or_skip(count * 16, 0, buffered()) else {
            return;
        };
        let data = pattern(count * 16);

        let result = device
            .write_batch(
                data.chunks(16)
                    .enumerate()
                    .map(|(i, chunk)| ((i * 16) as u64, chunk)),
            )
            .unwrap();

        assert_eq!(result.bytes_written, data.len() as u64);
        assert_eq!(result.operations_completed, count);
        assert_eq!(std::fs::read(temp.path()).unwrap(), data);
    }

    #[test]
    fn write_batch_with_a_failed_write_still_completes_the_rest() {
        let Some((temp, mut device)) = open_temp_or_skip(BLOCK, 0, buffered()) else {
            return;
        };
        let negative_offset = 1u64 << 63;

        let err = device
            .write_batch([
                (0, &b"head"[..]),
                (negative_offset, b"lost"),
                (100, b"tail"),
            ])
            .unwrap_err();

        assert!(matches!(err, PlatformError::IoUringError(_)));
        let contents = std::fs::read(temp.path()).unwrap();
        assert_eq!(&contents[..4], b"head");
        assert_eq!(&contents[100..104], b"tail");

        assert_eq!(device.write_at(200, b"next").unwrap(), 4);
        assert_eq!(&std::fs::read(temp.path()).unwrap()[200..204], b"next");
    }

    #[test]
    fn write_batch_rejected_for_alignment_writes_nothing() {
        let Some((temp, mut device)) = open_temp_or_skip(3 * BLOCK, 0xFF, direct()) else {
            return;
        };
        let storage = vec![0xAA; 2 * BLOCK];
        let block = aligned(&storage, BLOCK);

        let err = device
            .write_batch([(0, block), (BLOCK as u64 + 1, block)])
            .unwrap_err();
        assert!(matches!(err, PlatformError::AlignmentError(_)));
        drop(storage);

        let replacement = vec![0x11; 2 * BLOCK];
        let result = device
            .write_batch([(2 * BLOCK as u64, aligned(&replacement, BLOCK))])
            .unwrap();
        assert_eq!(result.bytes_written, BLOCK as u64);

        let contents = std::fs::read(temp.path()).unwrap();
        assert!(contents[..2 * BLOCK].iter().all(|&b| b == 0xFF));
        assert!(contents[2 * BLOCK..].iter().all(|&b| b == 0x11));
    }

    #[test]
    fn sync_after_writes_succeeds() {
        let Some((temp, mut device)) = open_temp_or_skip(BLOCK, 0, buffered()) else {
            return;
        };

        device.write_at(0, b"durable").unwrap();
        device.sync().unwrap();

        assert_eq!(&std::fs::read(temp.path()).unwrap()[..7], b"durable");
    }

    #[test]
    fn write_and_seek_traits_advance_the_position() {
        let Some((temp, mut device)) = open_temp_or_skip(64, b'.', buffered()) else {
            return;
        };

        device.write_all(b"hello ").unwrap();
        device.write_all(b"world").unwrap();
        assert_eq!(device.stream_position().unwrap(), 11);
        device.seek(SeekFrom::Start(6)).unwrap();
        device.write_all(b"WORLD").unwrap();
        device.flush().unwrap();

        device.rewind().unwrap();
        let mut head = [0u8; 12];
        device.read_exact(&mut head).unwrap();
        assert_eq!(&head, b"hello WORLD.");
        assert_eq!(&std::fs::read(temp.path()).unwrap()[..12], b"hello WORLD.");
    }

    #[test]
    fn info_reports_the_opened_file() {
        let Some((temp, device)) = open_temp_or_skip(3 * BLOCK, 0, buffered().block_size(512))
        else {
            return;
        };

        let info = device.info();

        assert_eq!(info.path, temp.path().to_str().unwrap());
        assert_eq!(info.size, 3 * BLOCK as u64);
        assert_eq!(info.block_size, 512);
        assert!(!info.direct_io);
    }

    #[test]
    fn open_missing_path_is_device_not_found() {
        let err = IoUringDevice::open("/nonexistent/engraver-uring", buffered())
            .err()
            .unwrap();
        assert!(matches!(err, PlatformError::DeviceNotFound(_)));
    }

    #[test]
    fn open_rejects_non_power_of_two_block_size_for_direct_io() {
        let temp = NamedTempFile::new().unwrap();
        let err = IoUringDevice::open(temp.path().to_str().unwrap(), direct().block_size(3000))
            .err()
            .unwrap();
        assert!(matches!(err, PlatformError::AlignmentError(_)));
    }

    #[test]
    fn parse_kernel_version_reads_release_strings() {
        assert_eq!(parse_kernel_version("7.0.0-43-generic\n"), Some((7, 0, 0)));
        assert_eq!(parse_kernel_version("6.10.14-arch1-1"), Some((6, 10, 14)));
        assert_eq!(parse_kernel_version("5.6-rc1"), Some((5, 6, 0)));
        assert_eq!(parse_kernel_version("linux"), None);
        assert_eq!(parse_kernel_version("x.y.z"), None);
    }

    #[test]
    fn kernel_is_supported_wherever_io_uring_is_available() {
        if is_io_uring_available() {
            assert!(is_kernel_supported());
        }
    }
}
