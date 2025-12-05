//! High-performance block writer with progress tracking
//!
//! This module provides the core writing engine for Engraver, handling:
//! - Block-based writing with configurable block sizes
//! - Progress callbacks with speed and ETA calculation
//! - Retry logic for transient errors
//! - Sync/flush management

use crate::error::{Error, Result};
use std::io::{Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Default block size for write operations (4 MB)
pub const DEFAULT_BLOCK_SIZE: usize = 4 * 1024 * 1024;

/// Minimum block size (4 KB)
pub const MIN_BLOCK_SIZE: usize = 4 * 1024;

/// Maximum block size (64 MB)
pub const MAX_BLOCK_SIZE: usize = 64 * 1024 * 1024;

/// Default number of retry attempts
pub const DEFAULT_RETRY_ATTEMPTS: u32 = 3;

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
}

impl Default for WriteConfig {
    fn default() -> Self {
        Self {
            block_size: DEFAULT_BLOCK_SIZE,
            sync_each_block: false,
            sync_on_complete: true,
            retry_attempts: DEFAULT_RETRY_ATTEMPTS,
            retry_delay: Duration::from_millis(100),
            verify: false,
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
        mut source: R,
        mut target: W,
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
                &mut target,
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

        // Verification if enabled
        let verified = if self.config.verify {
            // Verification would go here - for now just mark as not done
            None
        } else {
            None
        };

        Ok(WriteResult {
            bytes_written: progress.bytes_written,
            elapsed,
            average_speed,
            retry_count: progress.retry_count,
            verified,
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
        };

        assert_eq!(result.speed_display(), "50.0 MB/s");
    }
}
