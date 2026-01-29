//! High-performance block writer with progress tracking
//!
//! This module provides the core writing engine for Engraver, handling:
//! - Block-based writing with configurable block sizes
//! - Progress callbacks with speed and ETA calculation
//! - Retry logic for transient errors
//! - Sync/flush management

use crate::error::{Error, Result};
use crate::settings::{DEFAULT_RETRY_ATTEMPTS, DEFAULT_RETRY_DELAY_MS};
use crate::verifier::ChecksumAlgorithm;
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Trait alias for types that can be read and seeked (used for verification)
pub trait ReadSeek: Read + Seek {}
impl<T: Read + Seek> ReadSeek for T {}

/// Internal hasher enum for calculating checksums during write
#[cfg(feature = "checksum")]
enum SourceHasher {
    Sha256(sha2::Sha256),
    Sha512(sha2::Sha512),
    Md5(md5::Md5),
    Crc32(crc32fast::Hasher),
}

#[cfg(feature = "checksum")]
impl SourceHasher {
    fn update(&mut self, data: &[u8]) {
        use sha2::Digest;
        match self {
            SourceHasher::Sha256(h) => h.update(data),
            SourceHasher::Sha512(h) => h.update(data),
            SourceHasher::Md5(h) => h.update(data),
            SourceHasher::Crc32(h) => h.update(data),
        }
    }

    fn finalize_hex(self) -> String {
        use sha2::Digest;
        match self {
            SourceHasher::Sha256(h) => {
                let result = h.finalize();
                result.iter().map(|b| format!("{:02x}", b)).collect()
            }
            SourceHasher::Sha512(h) => {
                let result = h.finalize();
                result.iter().map(|b| format!("{:02x}", b)).collect()
            }
            SourceHasher::Md5(h) => {
                let result = h.finalize();
                result.iter().map(|b| format!("{:02x}", b)).collect()
            }
            SourceHasher::Crc32(h) => {
                format!("{:08x}", h.finalize())
            }
        }
    }
}

/// Default block size for write operations (4 MB)
pub const DEFAULT_BLOCK_SIZE: usize = 4 * 1024 * 1024;

/// Minimum block size (4 KB)
pub const MIN_BLOCK_SIZE: usize = 4 * 1024;

/// Maximum block size (64 MB)
pub const MAX_BLOCK_SIZE: usize = 64 * 1024 * 1024;

/// Write progress information
#[derive(Debug, Clone)]
pub struct WriteProgress {
    /// Bytes written so far
    pub bytes_written: u64,

    /// Total bytes to write
    pub total_bytes: u64,

    /// Current write speed in bytes per second
    pub speed_bps: u64,

    /// Estimated time remaining in seconds
    pub eta_seconds: Option<u64>,

    /// Current block number being written
    pub current_block: u64,

    /// Total number of blocks
    pub total_blocks: u64,

    /// Elapsed time since start
    pub elapsed: Duration,

    /// Number of retries that occurred
    pub retry_count: u32,
}

impl WriteProgress {
    /// Create a new progress instance
    pub fn new(total_bytes: u64, block_size: usize) -> Self {
        let total_blocks = total_bytes.div_ceil(block_size as u64);
        Self {
            bytes_written: 0,
            total_bytes,
            speed_bps: 0,
            eta_seconds: None,
            current_block: 0,
            total_blocks,
            elapsed: Duration::ZERO,
            retry_count: 0,
        }
    }

    /// Calculate completion percentage (0.0 to 100.0)
    pub fn percentage(&self) -> f64 {
        if self.total_bytes == 0 {
            100.0
        } else {
            (self.bytes_written as f64 / self.total_bytes as f64) * 100.0
        }
    }

    /// Check if write is complete
    pub fn is_complete(&self) -> bool {
        self.bytes_written >= self.total_bytes
    }

    /// Format speed for display (e.g., "45.2 MB/s")
    pub fn speed_display(&self) -> String {
        format_speed(self.speed_bps)
    }

    /// Format ETA for display (e.g., "2m 30s")
    pub fn eta_display(&self) -> String {
        match self.eta_seconds {
            Some(secs) if secs > 0 => format_duration(secs),
            _ => "calculating...".to_string(),
        }
    }
}

/// Progress callback type
pub type ProgressCallback = Box<dyn Fn(&WriteProgress) + Send + Sync>;

/// Configuration for write operations
#[derive(Debug, Clone)]
pub struct WriteConfig {
    /// Block size for read/write operations
    pub block_size: usize,

    /// Whether to sync after each block
    pub sync_each_block: bool,

    /// Whether to sync after write completes
    pub sync_on_complete: bool,

    /// Number of retry attempts on error
    pub retry_attempts: u32,

    /// Delay between retries
    pub retry_delay: Duration,

    /// Whether to verify writes (read back and compare)
    pub verify: bool,

    /// Checksum algorithm for parallel verification (calculated during write)
    /// When set, the checksum is computed while writing data and then verified
    /// by reading back the written data, avoiding a second read of the source.
    pub checksum_algorithm: Option<ChecksumAlgorithm>,
}

impl Default for WriteConfig {
    fn default() -> Self {
        Self {
            block_size: DEFAULT_BLOCK_SIZE,
            sync_each_block: false,
            sync_on_complete: true,
            retry_attempts: DEFAULT_RETRY_ATTEMPTS,
            retry_delay: Duration::from_millis(DEFAULT_RETRY_DELAY_MS),
            verify: false,
            checksum_algorithm: None,
        }
    }
}

impl WriteConfig {
    /// Create a new config with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Set block size (clamped to valid range)
    pub fn block_size(mut self, size: usize) -> Self {
        self.block_size = size.clamp(MIN_BLOCK_SIZE, MAX_BLOCK_SIZE);
        self
    }

    /// Set sync after each block
    pub fn sync_each_block(mut self, sync: bool) -> Self {
        self.sync_each_block = sync;
        self
    }

    /// Set sync on complete
    pub fn sync_on_complete(mut self, sync: bool) -> Self {
        self.sync_on_complete = sync;
        self
    }

    /// Set retry attempts
    pub fn retry_attempts(mut self, attempts: u32) -> Self {
        self.retry_attempts = attempts;
        self
    }

    /// Set retry delay
    pub fn retry_delay(mut self, delay: Duration) -> Self {
        self.retry_delay = delay;
        self
    }

    /// Set verify mode
    pub fn verify(mut self, verify: bool) -> Self {
        self.verify = verify;
        self
    }

    /// Set checksum algorithm for parallel verification
    ///
    /// When set, the writer will calculate a checksum of the source data
    /// during the write operation, then verify the written data by reading
    /// it back and comparing checksums. This avoids reading the source twice.
    pub fn checksum_algorithm(mut self, algorithm: Option<ChecksumAlgorithm>) -> Self {
        self.checksum_algorithm = algorithm;
        self
    }
}

/// Result of a write operation
#[derive(Debug, Clone)]
pub struct WriteResult {
    /// Total bytes written
    pub bytes_written: u64,

    /// Total time elapsed
    pub elapsed: Duration,

    /// Average speed in bytes per second
    pub average_speed: u64,

    /// Number of retries that occurred
    pub retry_count: u32,

    /// Whether verification passed (if enabled)
    pub verified: Option<bool>,

    /// Checksum calculated from source data during write (if checksum_algorithm was set)
    pub source_checksum: Option<String>,

    /// Checksum calculated from target data during verification (if verification was performed)
    pub target_checksum: Option<String>,

    /// Time spent on verification (if performed)
    pub verification_elapsed: Option<Duration>,
}

impl WriteResult {
    /// Format average speed for display
    pub fn speed_display(&self) -> String {
        format_speed(self.average_speed)
    }
}

/// Writer engine for block device operations
pub struct Writer {
    config: WriteConfig,
    progress_callback: Option<ProgressCallback>,
    cancel_flag: Arc<AtomicBool>,
}

impl Writer {
    /// Create a new writer with default configuration
    pub fn new() -> Self {
        Self {
            config: WriteConfig::default(),
            progress_callback: None,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Create a new writer with custom configuration
    pub fn with_config(config: WriteConfig) -> Self {
        Self {
            config,
            progress_callback: None,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Set a progress callback
    pub fn on_progress<F>(mut self, callback: F) -> Self
    where
        F: Fn(&WriteProgress) + Send + Sync + 'static,
    {
        self.progress_callback = Some(Box::new(callback));
        self
    }

    /// Get a handle to cancel the write operation
    pub fn cancel_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel_flag)
    }

    /// Write from source to target
    ///
    /// # Arguments
    /// * `source` - Readable source (file, network stream, etc.)
    /// * `target` - Writable target (device, file, etc.)
    /// * `source_size` - Total size of source in bytes
    ///
    /// # Returns
    /// * `Ok(WriteResult)` - Write completed successfully
    /// * `Err(Error)` - Write failed
    pub fn write<R, W>(&mut self, source: R, target: W, source_size: u64) -> Result<WriteResult>
    where
        R: Read,
        W: Write + Seek,
    {
        self.write_from_offset(source, target, source_size, 0)
    }

    /// Write from source to target, starting from a specific offset
    ///
    /// This is useful for resuming interrupted writes. The source must already
    /// be seeked to the correct position before calling this method.
    ///
    /// # Arguments
    /// * `source` - Readable source (already seeked to start_offset)
    /// * `target` - Writable target (device, file, etc.)
    /// * `source_size` - Total size of source in bytes
    /// * `start_offset` - Byte offset to start writing from
    ///
    /// # Returns
    /// * `Ok(WriteResult)` - Write completed successfully
    /// * `Err(Error)` - Write failed
    pub fn write_from_offset<R, W>(
        &mut self,
        source: R,
        mut target: W,
        source_size: u64,
        start_offset: u64,
    ) -> Result<WriteResult>
    where
        R: Read,
        W: Write + Seek,
    {
        self.write_internal(source, &mut target, source_size, start_offset)
    }

    /// Write from source to target with parallel verification
    ///
    /// This method calculates a checksum of the source data during the write operation,
    /// then reads back the written data to verify it matches. This is more efficient than
    /// a separate verification pass because the source only needs to be read once.
    ///
    /// Requires `checksum_algorithm` to be set in the config.
    ///
    /// # Arguments
    /// * `source` - Readable source
    /// * `target` - Target device (must be readable for verification)
    /// * `source_size` - Total size of source in bytes
    ///
    /// # Returns
    /// * `Ok(WriteResult)` - Write completed with verification results
    /// * `Err(Error)` - Write or verification failed
    #[cfg(feature = "checksum")]
    pub fn write_and_verify<R, W>(
        &mut self,
        source: R,
        mut target: W,
        source_size: u64,
    ) -> Result<WriteResult>
    where
        R: Read,
        W: Read + Write + Seek,
    {
        // First, write the data (this calculates source checksum if algorithm is set)
        let mut result = self.write_internal(source, &mut target, source_size, 0)?;

        // If we have a source checksum, verify by reading back the target
        if let Some(ref source_checksum) = result.source_checksum {
            let verify_start = Instant::now();

            // Seek back to start
            target.seek(SeekFrom::Start(0))?;

            // Calculate target checksum
            let algorithm = self.config.checksum_algorithm.ok_or_else(|| {
                Error::InvalidConfig("checksum_algorithm must be set for verification".to_string())
            })?;

            let target_checksum = self.calculate_checksum(&mut target, source_size, algorithm)?;

            result.verified = Some(&target_checksum == source_checksum);
            result.target_checksum = Some(target_checksum);
            result.verification_elapsed = Some(verify_start.elapsed());
        }

        Ok(result)
    }

    /// Calculate checksum of a reader (used for verification)
    #[cfg(feature = "checksum")]
    fn calculate_checksum<R: Read>(
        &self,
        reader: &mut R,
        size: u64,
        algorithm: ChecksumAlgorithm,
    ) -> Result<String> {
        use sha2::Digest;

        let mut hasher = match algorithm {
            ChecksumAlgorithm::Sha256 => SourceHasher::Sha256(sha2::Sha256::new()),
            ChecksumAlgorithm::Sha512 => SourceHasher::Sha512(sha2::Sha512::new()),
            ChecksumAlgorithm::Md5 => SourceHasher::Md5(md5::Md5::new()),
            ChecksumAlgorithm::Crc32 => SourceHasher::Crc32(crc32fast::Hasher::new()),
        };

        let block_size = self.config.block_size;
        let mut buffer = vec![0u8; block_size];
        let mut bytes_read_total = 0u64;

        while bytes_read_total < size {
            // Check for cancellation
            if self.cancel_flag.load(Ordering::SeqCst) {
                return Err(Error::Cancelled);
            }

            let to_read = block_size.min((size - bytes_read_total) as usize);
            let bytes_read = read_exact_or_eof(reader, &mut buffer[..to_read])?;

            if bytes_read == 0 {
                break;
            }

            hasher.update(&buffer[..bytes_read]);
            bytes_read_total += bytes_read as u64;
        }

        Ok(hasher.finalize_hex())
    }

    /// Internal write implementation with checksum calculation
    #[cfg(feature = "checksum")]
    fn write_internal<R, W>(
        &mut self,
        mut source: R,
        target: &mut W,
        source_size: u64,
        start_offset: u64,
    ) -> Result<WriteResult>
    where
        R: Read,
        W: Write + Seek,
    {
        use sha2::Digest;

        // Reset cancel flag
        self.cancel_flag.store(false, Ordering::SeqCst);

        let start_time = Instant::now();
        let block_size = self.config.block_size;

        let mut buffer = vec![0u8; block_size];
        let mut progress = WriteProgress::new(source_size, block_size);
        let mut speed_tracker = SpeedTracker::new();

        // Initialize progress with already-written bytes for resumed writes
        progress.bytes_written = start_offset;
        progress.current_block = start_offset / block_size as u64;

        // Create hasher if checksum algorithm is set
        let mut hasher: Option<SourceHasher> =
            self.config.checksum_algorithm.map(|alg| match alg {
                ChecksumAlgorithm::Sha256 => SourceHasher::Sha256(sha2::Sha256::new()),
                ChecksumAlgorithm::Sha512 => SourceHasher::Sha512(sha2::Sha512::new()),
                ChecksumAlgorithm::Md5 => SourceHasher::Md5(md5::Md5::new()),
                ChecksumAlgorithm::Crc32 => SourceHasher::Crc32(crc32fast::Hasher::new()),
            });

        // Seek target to the starting offset
        target.seek(SeekFrom::Start(start_offset))?;

        loop {
            // Check for cancellation
            if self.cancel_flag.load(Ordering::SeqCst) {
                return Err(Error::Cancelled);
            }

            // Read a block from source
            let bytes_read = read_exact_or_eof(&mut source, &mut buffer)?;

            if bytes_read == 0 {
                break; // EOF
            }

            // Update hasher with source data
            if let Some(ref mut h) = hasher {
                h.update(&buffer[..bytes_read]);
            }

            // Write the block with retry logic
            let write_result = self.write_block_with_retry(
                target,
                &buffer[..bytes_read],
                progress.bytes_written,
                &mut progress.retry_count,
            );

            match write_result {
                Ok(bytes_written) => {
                    progress.bytes_written += bytes_written as u64;
                    progress.current_block += 1;
                }
                Err(e) => {
                    return Err(e);
                }
            }

            // Sync if configured
            if self.config.sync_each_block {
                target.flush()?;
            }

            // Update progress
            progress.elapsed = start_time.elapsed();
            speed_tracker.update(progress.bytes_written);
            progress.speed_bps = speed_tracker.current_speed();
            progress.eta_seconds = calculate_eta(
                progress.bytes_written,
                progress.total_bytes,
                progress.speed_bps,
            );

            // Call progress callback
            if let Some(ref callback) = self.progress_callback {
                callback(&progress);
            }
        }

        // Final sync
        if self.config.sync_on_complete {
            target.flush()?;
        }

        let write_elapsed = start_time.elapsed();
        let average_speed = if write_elapsed.as_secs() > 0 {
            progress.bytes_written / write_elapsed.as_secs()
        } else {
            progress.bytes_written
        };

        // Get source checksum if calculated
        let source_checksum = hasher.map(|h| h.finalize_hex());

        Ok(WriteResult {
            bytes_written: progress.bytes_written,
            elapsed: write_elapsed,
            average_speed,
            retry_count: progress.retry_count,
            verified: None,
            source_checksum,
            target_checksum: None,
            verification_elapsed: None,
        })
    }

    /// Internal write implementation without checksum feature
    #[cfg(not(feature = "checksum"))]
    fn write_internal<R, W>(
        &mut self,
        mut source: R,
        target: &mut W,
        source_size: u64,
        start_offset: u64,
    ) -> Result<WriteResult>
    where
        R: Read,
        W: Write + Seek,
    {
        // Reset cancel flag
        self.cancel_flag.store(false, Ordering::SeqCst);

        let start_time = Instant::now();
        let block_size = self.config.block_size;

        let mut buffer = vec![0u8; block_size];
        let mut progress = WriteProgress::new(source_size, block_size);
        let mut speed_tracker = SpeedTracker::new();

        // Initialize progress with already-written bytes for resumed writes
        progress.bytes_written = start_offset;
        progress.current_block = start_offset / block_size as u64;

        // Seek target to the starting offset
        target.seek(SeekFrom::Start(start_offset))?;

        loop {
            // Check for cancellation
            if self.cancel_flag.load(Ordering::SeqCst) {
                return Err(Error::Cancelled);
            }

            // Read a block from source
            let bytes_read = read_exact_or_eof(&mut source, &mut buffer)?;

            if bytes_read == 0 {
                break; // EOF
            }

            // Write the block with retry logic
            let write_result = self.write_block_with_retry(
                target,
                &buffer[..bytes_read],
                progress.bytes_written,
                &mut progress.retry_count,
            );

            match write_result {
                Ok(bytes_written) => {
                    progress.bytes_written += bytes_written as u64;
                    progress.current_block += 1;
                }
                Err(e) => {
                    return Err(e);
                }
            }

            // Sync if configured
            if self.config.sync_each_block {
                target.flush()?;
            }

            // Update progress
            progress.elapsed = start_time.elapsed();
            speed_tracker.update(progress.bytes_written);
            progress.speed_bps = speed_tracker.current_speed();
            progress.eta_seconds = calculate_eta(
                progress.bytes_written,
                progress.total_bytes,
                progress.speed_bps,
            );

            // Call progress callback
            if let Some(ref callback) = self.progress_callback {
                callback(&progress);
            }
        }

        // Final sync
        if self.config.sync_on_complete {
            target.flush()?;
        }

        let elapsed = start_time.elapsed();
        let average_speed = if elapsed.as_secs() > 0 {
            progress.bytes_written / elapsed.as_secs()
        } else {
            progress.bytes_written
        };

        Ok(WriteResult {
            bytes_written: progress.bytes_written,
            elapsed,
            average_speed,
            retry_count: progress.retry_count,
            verified: None,
            source_checksum: None,
            target_checksum: None,
            verification_elapsed: None,
        })
    }

    /// Write a single block with retry logic
    fn write_block_with_retry<W: Write + Seek>(
        &self,
        target: &mut W,
        data: &[u8],
        offset: u64,
        retry_count: &mut u32,
    ) -> Result<usize> {
        let mut last_error = None;

        for attempt in 0..=self.config.retry_attempts {
            if attempt > 0 {
                *retry_count += 1;
                std::thread::sleep(self.config.retry_delay);

                // Seek back to the write position
                target.seek(SeekFrom::Start(offset))?;
            }

            match target.write(data) {
                Ok(n) if n == data.len() => return Ok(n),
                Ok(n) => {
                    // Partial write - this is an error for block devices
                    last_error = Some(Error::PartialWrite {
                        expected: data.len(),
                        actual: n,
                    });
                }
                Err(e) => {
                    last_error = Some(Error::Io(e));
                }
            }
        }

        Err(last_error.unwrap_or_else(|| Error::Unknown("Write failed".to_string())))
    }
}

impl Default for Writer {
    fn default() -> Self {
        Self::new()
    }
}

/// Speed tracking with smoothing
struct SpeedTracker {
    samples: Vec<(Instant, u64)>,
    max_samples: usize,
}

impl SpeedTracker {
    fn new() -> Self {
        Self {
            samples: Vec::with_capacity(10),
            max_samples: 10,
        }
    }

    fn update(&mut self, bytes_written: u64) {
        let now = Instant::now();

        if self.samples.len() >= self.max_samples {
            self.samples.remove(0);
        }

        self.samples.push((now, bytes_written));
    }

    fn current_speed(&self) -> u64 {
        if self.samples.len() < 2 {
            return 0;
        }

        let first = &self.samples[0];
        let last = &self.samples[self.samples.len() - 1];

        let duration = last.0.duration_since(first.0);
        let bytes = last.1.saturating_sub(first.1);

        if duration.as_millis() > 0 {
            (bytes as f64 / duration.as_secs_f64()) as u64
        } else {
            0
        }
    }
}

/// Read exactly the buffer size or until EOF
fn read_exact_or_eof<R: Read>(reader: &mut R, buffer: &mut [u8]) -> Result<usize> {
    let mut total_read = 0;

    while total_read < buffer.len() {
        match reader.read(&mut buffer[total_read..]) {
            Ok(0) => break, // EOF
            Ok(n) => total_read += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(Error::Io(e)),
        }
    }

    Ok(total_read)
}

/// Calculate estimated time remaining
fn calculate_eta(bytes_written: u64, total_bytes: u64, speed_bps: u64) -> Option<u64> {
    if speed_bps == 0 || bytes_written >= total_bytes {
        return None;
    }

    let remaining = total_bytes.saturating_sub(bytes_written);
    Some(remaining / speed_bps)
}

/// Format speed for display
pub fn format_speed(bytes_per_second: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes_per_second >= GB {
        format!("{:.1} GB/s", bytes_per_second as f64 / GB as f64)
    } else if bytes_per_second >= MB {
        format!("{:.1} MB/s", bytes_per_second as f64 / MB as f64)
    } else if bytes_per_second >= KB {
        format!("{:.1} KB/s", bytes_per_second as f64 / KB as f64)
    } else {
        format!("{} B/s", bytes_per_second)
    }
}

/// Format duration for display
pub fn format_duration(seconds: u64) -> String {
    if seconds >= 3600 {
        let hours = seconds / 3600;
        let mins = (seconds % 3600) / 60;
        format!("{}h {}m", hours, mins)
    } else if seconds >= 60 {
        let mins = seconds / 60;
        let secs = seconds % 60;
        format!("{}m {}s", mins, secs)
    } else {
        format!("{}s", seconds)
    }
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::atomic::AtomicU64;

    // -------------------------------------------------------------------------
    // WriteProgress tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_write_progress_new() {
        let progress = WriteProgress::new(1024 * 1024, 4096);
        assert_eq!(progress.bytes_written, 0);
        assert_eq!(progress.total_bytes, 1024 * 1024);
        assert_eq!(progress.total_blocks, 256);
        assert!(!progress.is_complete());
    }

    #[test]
    fn test_write_progress_percentage() {
        let mut progress = WriteProgress::new(1000, 100);

        assert_eq!(progress.percentage(), 0.0);

        progress.bytes_written = 500;
        assert_eq!(progress.percentage(), 50.0);

        progress.bytes_written = 1000;
        assert_eq!(progress.percentage(), 100.0);
    }

    #[test]
    fn test_write_progress_percentage_zero_total() {
        let progress = WriteProgress::new(0, 4096);
        assert_eq!(progress.percentage(), 100.0);
    }

    #[test]
    fn test_write_progress_is_complete() {
        let mut progress = WriteProgress::new(1000, 100);
        assert!(!progress.is_complete());

        progress.bytes_written = 999;
        assert!(!progress.is_complete());

        progress.bytes_written = 1000;
        assert!(progress.is_complete());

        progress.bytes_written = 1001;
        assert!(progress.is_complete());
    }

    #[test]
    fn test_write_progress_speed_display() {
        let mut progress = WriteProgress::new(1000, 100);

        progress.speed_bps = 1024;
        assert_eq!(progress.speed_display(), "1.0 KB/s");

        progress.speed_bps = 10 * 1024 * 1024;
        assert_eq!(progress.speed_display(), "10.0 MB/s");
    }

    #[test]
    fn test_write_progress_eta_display() {
        let mut progress = WriteProgress::new(1000, 100);

        progress.eta_seconds = None;
        assert_eq!(progress.eta_display(), "calculating...");

        progress.eta_seconds = Some(0);
        assert_eq!(progress.eta_display(), "calculating...");

        progress.eta_seconds = Some(30);
        assert_eq!(progress.eta_display(), "30s");

        progress.eta_seconds = Some(90);
        assert_eq!(progress.eta_display(), "1m 30s");
    }

    // -------------------------------------------------------------------------
    // WriteConfig tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_write_config_default() {
        let config = WriteConfig::default();
        assert_eq!(config.block_size, DEFAULT_BLOCK_SIZE);
        assert!(!config.sync_each_block);
        assert!(config.sync_on_complete);
        assert_eq!(config.retry_attempts, DEFAULT_RETRY_ATTEMPTS);
    }

    #[test]
    fn test_write_config_builder() {
        let config = WriteConfig::new()
            .block_size(1024 * 1024)
            .sync_each_block(true)
            .sync_on_complete(false)
            .retry_attempts(5)
            .verify(true);

        assert_eq!(config.block_size, 1024 * 1024);
        assert!(config.sync_each_block);
        assert!(!config.sync_on_complete);
        assert_eq!(config.retry_attempts, 5);
        assert!(config.verify);
    }

    #[test]
    fn test_write_config_block_size_clamping() {
        // Too small
        let config = WriteConfig::new().block_size(100);
        assert_eq!(config.block_size, MIN_BLOCK_SIZE);

        // Too large
        let config = WriteConfig::new().block_size(1024 * 1024 * 1024);
        assert_eq!(config.block_size, MAX_BLOCK_SIZE);

        // Just right
        let config = WriteConfig::new().block_size(1024 * 1024);
        assert_eq!(config.block_size, 1024 * 1024);
    }

    // -------------------------------------------------------------------------
    // Format functions tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_format_speed() {
        assert_eq!(format_speed(0), "0 B/s");
        assert_eq!(format_speed(512), "512 B/s");
        assert_eq!(format_speed(1024), "1.0 KB/s");
        assert_eq!(format_speed(1536), "1.5 KB/s");
        assert_eq!(format_speed(1024 * 1024), "1.0 MB/s");
        assert_eq!(format_speed(50 * 1024 * 1024), "50.0 MB/s");
        assert_eq!(format_speed(1024 * 1024 * 1024), "1.0 GB/s");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(0), "0s");
        assert_eq!(format_duration(30), "30s");
        assert_eq!(format_duration(59), "59s");
        assert_eq!(format_duration(60), "1m 0s");
        assert_eq!(format_duration(90), "1m 30s");
        assert_eq!(format_duration(3600), "1h 0m");
        assert_eq!(format_duration(3661), "1h 1m");
        assert_eq!(format_duration(7200), "2h 0m");
    }

    // -------------------------------------------------------------------------
    // calculate_eta tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_calculate_eta() {
        // No speed
        assert_eq!(calculate_eta(0, 1000, 0), None);

        // Already complete
        assert_eq!(calculate_eta(1000, 1000, 100), None);

        // Normal case
        assert_eq!(calculate_eta(500, 1000, 100), Some(5));

        // Just started
        assert_eq!(calculate_eta(0, 1000, 100), Some(10));
    }

    // -------------------------------------------------------------------------
    // read_exact_or_eof tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_read_exact_or_eof_full_read() {
        let data = vec![1u8, 2, 3, 4, 5];
        let mut cursor = Cursor::new(data);
        let mut buffer = vec![0u8; 5];

        let n = read_exact_or_eof(&mut cursor, &mut buffer).unwrap();
        assert_eq!(n, 5);
        assert_eq!(buffer, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_read_exact_or_eof_partial_read() {
        let data = vec![1u8, 2, 3];
        let mut cursor = Cursor::new(data);
        let mut buffer = vec![0u8; 5];

        let n = read_exact_or_eof(&mut cursor, &mut buffer).unwrap();
        assert_eq!(n, 3);
        assert_eq!(&buffer[..3], &[1, 2, 3]);
    }

    #[test]
    fn test_read_exact_or_eof_empty() {
        let data: Vec<u8> = vec![];
        let mut cursor = Cursor::new(data);
        let mut buffer = vec![0u8; 5];

        let n = read_exact_or_eof(&mut cursor, &mut buffer).unwrap();
        assert_eq!(n, 0);
    }

    // -------------------------------------------------------------------------
    // Writer tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_writer_new() {
        let writer = Writer::new();
        assert_eq!(writer.config.block_size, DEFAULT_BLOCK_SIZE);
    }

    #[test]
    fn test_writer_with_config() {
        let config = WriteConfig::new().block_size(8192);
        let writer = Writer::with_config(config);
        assert_eq!(writer.config.block_size, 8192);
    }

    #[test]
    fn test_writer_simple_write() {
        let source_data = vec![0xABu8; 1024];
        let source = Cursor::new(source_data.clone());
        let target = Cursor::new(vec![0u8; 1024]);

        let config = WriteConfig::new().block_size(256);
        let mut writer = Writer::with_config(config);

        let result = writer.write(source, target, 1024).unwrap();

        assert_eq!(result.bytes_written, 1024);
        assert_eq!(result.retry_count, 0);
    }

    #[test]
    fn test_writer_with_progress() {
        // Use 4 blocks worth of data at MIN_BLOCK_SIZE (4096 * 4 = 16384)
        let data_size = MIN_BLOCK_SIZE * 4;
        let source_data = vec![0xABu8; data_size];
        let source = Cursor::new(source_data);
        let target = Cursor::new(vec![0u8; data_size]);

        let progress_count = Arc::new(AtomicU64::new(0));
        let progress_count_clone = Arc::clone(&progress_count);

        let config = WriteConfig::new().block_size(MIN_BLOCK_SIZE);
        let mut writer = Writer::with_config(config).on_progress(move |_progress| {
            progress_count_clone.fetch_add(1, Ordering::SeqCst);
        });

        let _result = writer.write(source, target, data_size as u64).unwrap();

        // Should have 4 progress callbacks (one per block)
        assert_eq!(progress_count.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn test_writer_verify_data_integrity() {
        let source_data: Vec<u8> = (0..4096).map(|i| (i % 256) as u8).collect();
        let source = Cursor::new(source_data.clone());
        let mut target = Cursor::new(vec![0u8; 4096]);

        let config = WriteConfig::new().block_size(1024);
        let mut writer = Writer::with_config(config);

        let _result = writer.write(source, &mut target, 4096).unwrap();

        // Verify target has correct data
        assert_eq!(target.into_inner(), source_data);
    }

    #[test]
    fn test_writer_cancel() {
        // Use enough data for multiple blocks
        let data_size = MIN_BLOCK_SIZE * 10;
        let source_data = vec![0xABu8; data_size];
        let source = Cursor::new(source_data);
        let target = Cursor::new(vec![0u8; data_size]);

        let config = WriteConfig::new().block_size(MIN_BLOCK_SIZE);
        let writer = Writer::with_config(config);

        let cancel_handle = writer.cancel_handle();
        let cancel_clone = Arc::clone(&cancel_handle);

        // Cancel after first block via progress callback
        let writer = writer.on_progress(move |progress| {
            if progress.current_block >= 1 {
                cancel_clone.store(true, Ordering::SeqCst);
            }
        });

        let mut writer = writer;
        let result = writer.write(source, target, data_size as u64);

        assert!(matches!(result, Err(Error::Cancelled)));
    }

    #[test]
    fn test_writer_empty_source() {
        let source = Cursor::new(Vec::<u8>::new());
        let target = Cursor::new(vec![0u8; 1024]);

        let mut writer = Writer::new();
        let result = writer.write(source, target, 0).unwrap();

        assert_eq!(result.bytes_written, 0);
    }

    // -------------------------------------------------------------------------
    // SpeedTracker tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_speed_tracker_empty() {
        let tracker = SpeedTracker::new();
        assert_eq!(tracker.current_speed(), 0);
    }

    #[test]
    fn test_speed_tracker_single_sample() {
        let mut tracker = SpeedTracker::new();
        tracker.update(1000);
        assert_eq!(tracker.current_speed(), 0); // Need at least 2 samples
    }

    #[test]
    fn test_speed_tracker_multiple_samples() {
        // Skip timing-sensitive tests in CI where thread scheduling is unpredictable
        if std::env::var("CI").is_ok() {
            return;
        }

        let mut tracker = SpeedTracker::new();

        tracker.update(0);
        std::thread::sleep(Duration::from_millis(100));
        tracker.update(100_000);

        let speed = tracker.current_speed();
        // Should be roughly 1 MB/s (100KB in 100ms)
        assert!(speed > 500_000 && speed < 2_000_000, "Speed was {}", speed);
    }

    // -------------------------------------------------------------------------
    // WriteResult tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_write_result_speed_display() {
        let result = WriteResult {
            bytes_written: 1024 * 1024,
            elapsed: Duration::from_secs(1),
            average_speed: 50 * 1024 * 1024,
            retry_count: 0,
            verified: None,
            source_checksum: None,
            target_checksum: None,
            verification_elapsed: None,
        };

        assert_eq!(result.speed_display(), "50.0 MB/s");
    }

    // -------------------------------------------------------------------------
    // Parallel verification tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_write_with_checksum_algorithm_calculates_source_checksum() {
        use crate::verifier::ChecksumAlgorithm;

        let source_data = vec![0xABu8; 1024];
        let source = Cursor::new(source_data);
        let target = Cursor::new(vec![0u8; 1024]);

        let config = WriteConfig::new()
            .block_size(MIN_BLOCK_SIZE)
            .checksum_algorithm(Some(ChecksumAlgorithm::Sha256));
        let mut writer = Writer::with_config(config);

        let result = writer.write(source, target, 1024).unwrap();

        assert_eq!(result.bytes_written, 1024);
        assert!(result.source_checksum.is_some());
        // SHA-256 produces 64 hex characters
        assert_eq!(result.source_checksum.as_ref().unwrap().len(), 64);
    }

    #[test]
    fn test_write_without_checksum_algorithm_no_checksum() {
        let source_data = vec![0xABu8; 1024];
        let source = Cursor::new(source_data);
        let target = Cursor::new(vec![0u8; 1024]);

        let config = WriteConfig::new().block_size(MIN_BLOCK_SIZE);
        let mut writer = Writer::with_config(config);

        let result = writer.write(source, target, 1024).unwrap();

        assert_eq!(result.bytes_written, 1024);
        assert!(result.source_checksum.is_none());
    }

    #[test]
    fn test_write_and_verify_success() {
        use crate::verifier::ChecksumAlgorithm;

        let source_data = vec![0xABu8; 4096];
        let source = Cursor::new(source_data);
        let target = Cursor::new(vec![0u8; 4096]);

        let config = WriteConfig::new()
            .block_size(MIN_BLOCK_SIZE)
            .checksum_algorithm(Some(ChecksumAlgorithm::Sha256));
        let mut writer = Writer::with_config(config);

        let result = writer.write_and_verify(source, target, 4096).unwrap();

        assert_eq!(result.bytes_written, 4096);
        assert!(result.source_checksum.is_some());
        assert!(result.target_checksum.is_some());
        assert_eq!(result.source_checksum, result.target_checksum);
        assert_eq!(result.verified, Some(true));
        assert!(result.verification_elapsed.is_some());
    }

    #[test]
    fn test_write_and_verify_with_md5() {
        use crate::verifier::ChecksumAlgorithm;

        let source_data = vec![0x42u8; 2048];
        let source = Cursor::new(source_data);
        let target = Cursor::new(vec![0u8; 2048]);

        let config = WriteConfig::new()
            .block_size(MIN_BLOCK_SIZE)
            .checksum_algorithm(Some(ChecksumAlgorithm::Md5));
        let mut writer = Writer::with_config(config);

        let result = writer.write_and_verify(source, target, 2048).unwrap();

        assert_eq!(result.bytes_written, 2048);
        assert!(result.source_checksum.is_some());
        // MD5 produces 32 hex characters
        assert_eq!(result.source_checksum.as_ref().unwrap().len(), 32);
        assert_eq!(result.verified, Some(true));
    }

    #[test]
    fn test_write_and_verify_with_crc32() {
        use crate::verifier::ChecksumAlgorithm;

        let source_data = vec![0x55u8; 1024];
        let source = Cursor::new(source_data);
        let target = Cursor::new(vec![0u8; 1024]);

        let config = WriteConfig::new()
            .block_size(MIN_BLOCK_SIZE)
            .checksum_algorithm(Some(ChecksumAlgorithm::Crc32));
        let mut writer = Writer::with_config(config);

        let result = writer.write_and_verify(source, target, 1024).unwrap();

        assert_eq!(result.bytes_written, 1024);
        assert!(result.source_checksum.is_some());
        // CRC32 produces 8 hex characters
        assert_eq!(result.source_checksum.as_ref().unwrap().len(), 8);
        assert_eq!(result.verified, Some(true));
    }

    #[test]
    fn test_checksum_config_builder() {
        use crate::verifier::ChecksumAlgorithm;

        let config = WriteConfig::new().checksum_algorithm(Some(ChecksumAlgorithm::Sha512));
        assert_eq!(config.checksum_algorithm, Some(ChecksumAlgorithm::Sha512));

        let config = WriteConfig::new().checksum_algorithm(None);
        assert_eq!(config.checksum_algorithm, None);
    }
}
