//! Benchmark module for testing drive write speeds.
//!
//! This module provides functionality to benchmark write speeds of storage devices,
//! helping users identify slow drives or connections before committing to long write operations.

use std::io::{Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use thiserror::Error;

// Constants
const MIN_BLOCK_SIZE: u64 = 4 * 1024; // 4 KB
const MAX_BLOCK_SIZE: u64 = 64 * 1024 * 1024; // 64 MB
const MIN_BLOCKS_PER_PASS: u64 = 10;
const DEFAULT_TEST_SIZE: u64 = 256 * 1024 * 1024; // 256 MB

/// Errors that can occur during benchmark operations
#[derive(Error, Debug)]
pub enum BenchmarkError {
    /// Size value is not a power of 2
    #[error("Size must be a power of 2: {0}")]
    NotPowerOfTwo(String),

    /// Block size exceeds maximum allowed (64 MB)
    #[error("Block size {0} exceeds maximum of 64 MB")]
    BlockSizeTooLarge(String),

    /// Block size is below minimum allowed (4 KB)
    #[error("Block size {0} is below minimum of 4 KB")]
    BlockSizeTooSmall(String),

    /// Both --size and --test-block-sizes were specified
    #[error("Cannot use both --size and --test-block-sizes options")]
    MutuallyExclusiveOptions,

    /// Invalid size format string
    #[error("Invalid size format: {0}")]
    InvalidSizeFormat(String),

    /// I/O error during benchmark
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Benchmark was cancelled by user
    #[error("Benchmark cancelled")]
    Cancelled,
}

/// Result type for benchmark operations
pub type Result<T> = std::result::Result<T, BenchmarkError>;

/// Data pattern for benchmark writes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DataPattern {
    /// All zeros (fastest to generate)
    #[default]
    Zeros,
    /// Random data (tests true write performance)
    Random,
    /// Sequential byte pattern (0x00, 0x01, ..., 0xFF, repeat)
    Sequential,
}

impl std::str::FromStr for DataPattern {
    type Err = BenchmarkError;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "zeros" | "zero" => Ok(DataPattern::Zeros),
            "random" => Ok(DataPattern::Random),
            "sequential" | "seq" => Ok(DataPattern::Sequential),
            _ => Err(BenchmarkError::InvalidSizeFormat(format!(
                "Unknown pattern '{}'. Use: zeros, random, or sequential",
                s
            ))),
        }
    }
}

/// Configuration for benchmark operations
#[derive(Debug, Clone)]
pub struct BenchmarkConfig {
    /// Total bytes to write (user-specified, may be adjusted)
    pub test_size: u64,
    /// Block size for write operations
    pub block_size: u64,
    /// Data pattern to write
    pub pattern: DataPattern,
    /// Number of benchmark passes
    pub passes: u32,
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            test_size: DEFAULT_TEST_SIZE,
            block_size: 4 * 1024 * 1024, // 4 MB
            pattern: DataPattern::Zeros,
            passes: 1,
        }
    }
}

impl BenchmarkConfig {
    /// Create a new benchmark configuration
    pub fn new(test_size: u64, block_size: u64) -> Self {
        Self {
            test_size,
            block_size,
            ..Default::default()
        }
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        // Check block size is power of 2
        if !is_power_of_two(self.block_size) {
            return Err(BenchmarkError::NotPowerOfTwo(format_size(self.block_size)));
        }

        // Check test size is power of 2
        if !is_power_of_two(self.test_size) {
            return Err(BenchmarkError::NotPowerOfTwo(format_size(self.test_size)));
        }

        // Check block size bounds
        if self.block_size > MAX_BLOCK_SIZE {
            return Err(BenchmarkError::BlockSizeTooLarge(format_size(
                self.block_size,
            )));
        }

        if self.block_size < MIN_BLOCK_SIZE {
            return Err(BenchmarkError::BlockSizeTooSmall(format_size(
                self.block_size,
            )));
        }

        Ok(())
    }

    /// Calculate the effective test size ensuring minimum blocks per pass
    pub fn effective_test_size(&self) -> u64 {
        let min_size = self.block_size * MIN_BLOCKS_PER_PASS;
        self.test_size.max(min_size)
    }

    /// Calculate effective test size for multi-block-size testing
    pub fn effective_test_size_for_block_sizes(base_size: u64, block_sizes: &[u64]) -> u64 {
        let largest_block = block_sizes.iter().copied().max().unwrap_or(0);
        let min_size = largest_block * MIN_BLOCKS_PER_PASS;
        base_size.max(min_size)
    }
}

/// Progress information during benchmark
#[derive(Debug, Clone)]
pub struct BenchmarkProgress {
    /// Bytes written so far
    pub bytes_written: u64,
    /// Total bytes to write
    pub total_bytes: u64,
    /// Current pass number (1-indexed)
    pub current_pass: u32,
    /// Total number of passes
    pub total_passes: u32,
    /// Current write speed in bytes per second
    pub current_speed_bps: u64,
    /// Time elapsed since benchmark start
    pub elapsed: Duration,
}

impl BenchmarkProgress {
    /// Get completion percentage (0-100)
    pub fn percentage(&self) -> u8 {
        if self.total_bytes == 0 {
            return 0;
        }
        ((self.bytes_written * 100) / self.total_bytes).min(100) as u8
    }

    /// Format speed for display
    pub fn speed_display(&self) -> String {
        format_speed(self.current_speed_bps)
    }
}

/// Result of a single benchmark pass
#[derive(Debug, Clone, Serialize)]
pub struct PassResult {
    /// Pass number (1-indexed)
    pub pass_number: u32,
    /// Bytes written in this pass
    pub bytes_written: u64,
    /// Block size used
    pub block_size: u64,
    /// Time elapsed for this pass
    #[serde(with = "duration_serde")]
    pub elapsed: Duration,
    /// Average speed in bytes per second
    pub average_speed_bps: u64,
    /// Minimum speed observed (bytes per second)
    pub min_speed_bps: u64,
    /// Maximum speed observed (bytes per second)
    pub max_speed_bps: u64,
}

/// Complete benchmark results
#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkResult {
    /// Device path that was benchmarked
    pub device_path: String,
    /// Test size used in bytes
    pub test_size: u64,
    /// Block size used in bytes
    pub block_size: u64,
    /// Data pattern used for the benchmark
    #[serde(serialize_with = "serialize_pattern")]
    pub pattern: DataPattern,
    /// Results from each pass
    pub passes: Vec<PassResult>,
    /// Overall summary
    pub summary: BenchmarkSummary,
}

fn serialize_pattern<S>(
    pattern: &DataPattern,
    serializer: S,
) -> std::result::Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let s = match pattern {
        DataPattern::Zeros => "zeros",
        DataPattern::Random => "random",
        DataPattern::Sequential => "sequential",
    };
    serializer.serialize_str(s)
}

/// Summary statistics across all passes
#[derive(Debug, Clone, Serialize)]
pub struct BenchmarkSummary {
    /// Total bytes written across all passes
    pub total_bytes_written: u64,
    /// Total time elapsed
    #[serde(with = "duration_serde")]
    pub total_elapsed: Duration,
    /// Average speed across all passes (bytes per second)
    pub average_speed_bps: u64,
    /// Minimum speed observed (bytes per second)
    pub min_speed_bps: u64,
    /// Maximum speed observed (bytes per second)
    pub max_speed_bps: u64,
}

/// Result when testing multiple block sizes
#[derive(Debug, Clone, Serialize)]
pub struct BlockSizeTestResult {
    /// Block size tested
    pub block_size: u64,
    /// Human-readable block size
    pub block_size_display: String,
    /// Average speed achieved (bytes per second)
    pub average_speed_bps: u64,
    /// Human-readable speed
    pub speed_display: String,
}

/// Serde helper for Duration
mod duration_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        duration.as_secs_f64().serialize(serializer)
    }

    #[allow(dead_code)]
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let secs = f64::deserialize(deserializer)?;
        Ok(Duration::from_secs_f64(secs))
    }
}

/// Speed tracker for calculating min/max/average speeds
struct SpeedTracker {
    samples: Vec<(Instant, u64)>,
    min_speed: Option<u64>,
    max_speed: Option<u64>,
}

impl SpeedTracker {
    fn new() -> Self {
        Self {
            samples: Vec::with_capacity(100),
            min_speed: None,
            max_speed: None,
        }
    }

    fn update(&mut self, bytes_written: u64) {
        let now = Instant::now();
        self.samples.push((now, bytes_written));

        // Calculate current speed from recent samples
        if self.samples.len() >= 2 {
            let current = self.current_speed();
            if current > 0 {
                self.min_speed = Some(self.min_speed.map_or(current, |m| m.min(current)));
                self.max_speed = Some(self.max_speed.map_or(current, |m| m.max(current)));
            }
        }
    }

    fn current_speed(&self) -> u64 {
        if self.samples.len() < 2 {
            return 0;
        }

        // Use last 10 samples for smoothing
        let start_idx = self.samples.len().saturating_sub(10);
        let (start_time, start_bytes) = self.samples[start_idx];
        let (end_time, end_bytes) = self.samples[self.samples.len() - 1];

        let elapsed = end_time.duration_since(start_time);
        if elapsed.is_zero() {
            return 0;
        }

        let bytes_diff = end_bytes.saturating_sub(start_bytes);
        (bytes_diff as f64 / elapsed.as_secs_f64()) as u64
    }

    fn min_speed(&self) -> u64 {
        self.min_speed.unwrap_or(0)
    }

    fn max_speed(&self) -> u64 {
        self.max_speed.unwrap_or(0)
    }
}

/// Data source for benchmark writes
struct BenchmarkDataSource {
    buffer: Vec<u8>,
}

impl BenchmarkDataSource {
    fn new(pattern: DataPattern, block_size: usize) -> Self {
        let buffer = match pattern {
            DataPattern::Zeros => vec![0u8; block_size],
            DataPattern::Random => {
                let mut buf = vec![0u8; block_size];
                // Simple pseudo-random fill (good enough for benchmarking)
                for (i, byte) in buf.iter_mut().enumerate() {
                    *byte = ((i * 1103515245 + 12345) >> 16) as u8;
                }
                buf
            }
            DataPattern::Sequential => (0..block_size).map(|i| (i % 256) as u8).collect(),
        };
        Self { buffer }
    }

    fn get_block(&self) -> &[u8] {
        &self.buffer
    }
}

/// Benchmark runner
pub struct BenchmarkRunner {
    config: BenchmarkConfig,
    cancel_flag: Arc<AtomicBool>,
}

impl BenchmarkRunner {
    /// Create a new benchmark runner
    pub fn new(config: BenchmarkConfig) -> Self {
        Self {
            config,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Get a handle to cancel the benchmark
    pub fn cancel_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel_flag)
    }

    /// Run the benchmark on a writable target
    pub fn run<W, F>(
        &self,
        mut target: W,
        device_path: &str,
        progress_callback: Option<F>,
    ) -> Result<BenchmarkResult>
    where
        W: Write + Seek,
        F: Fn(&BenchmarkProgress),
    {
        self.config.validate()?;

        let effective_size = self.config.effective_test_size();
        let block_size = self.config.block_size as usize;
        let data_source = BenchmarkDataSource::new(self.config.pattern, block_size);

        let mut passes = Vec::with_capacity(self.config.passes as usize);
        let total_bytes_all_passes = effective_size * self.config.passes as u64;

        for pass in 1..=self.config.passes {
            // Seek to beginning for each pass
            target.seek(SeekFrom::Start(0))?;

            let pass_result = self.run_pass(
                &mut target,
                &data_source,
                effective_size,
                pass,
                total_bytes_all_passes,
                (pass - 1) as u64 * effective_size,
                &progress_callback,
            )?;

            passes.push(pass_result);
        }

        // Calculate summary
        let summary = self.calculate_summary(&passes);

        Ok(BenchmarkResult {
            device_path: device_path.to_string(),
            test_size: effective_size,
            block_size: self.config.block_size,
            pattern: self.config.pattern,
            passes,
            summary,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn run_pass<W, F>(
        &self,
        target: &mut W,
        data_source: &BenchmarkDataSource,
        pass_size: u64,
        pass_number: u32,
        total_bytes_all_passes: u64,
        bytes_before_this_pass: u64,
        progress_callback: &Option<F>,
    ) -> Result<PassResult>
    where
        W: Write,
        F: Fn(&BenchmarkProgress),
    {
        let block_size = self.config.block_size as usize;
        let mut bytes_written: u64 = 0;
        let mut speed_tracker = SpeedTracker::new();
        let start_time = Instant::now();

        speed_tracker.update(0);

        while bytes_written < pass_size {
            // Check for cancellation
            if self.cancel_flag.load(Ordering::Relaxed) {
                return Err(BenchmarkError::Cancelled);
            }

            // Calculate bytes to write this iteration
            let remaining = pass_size - bytes_written;
            let to_write = (block_size as u64).min(remaining) as usize;
            let block = &data_source.get_block()[..to_write];

            // Write block
            target.write_all(block)?;
            bytes_written += to_write as u64;

            // Update speed tracker
            speed_tracker.update(bytes_written);

            // Report progress
            if let Some(ref callback) = progress_callback {
                let total_bytes_written = bytes_before_this_pass + bytes_written;
                callback(&BenchmarkProgress {
                    bytes_written: total_bytes_written,
                    total_bytes: total_bytes_all_passes,
                    current_pass: pass_number,
                    total_passes: self.config.passes,
                    current_speed_bps: speed_tracker.current_speed(),
                    elapsed: start_time.elapsed(),
                });
            }
        }

        // Flush to ensure data is written
        target.flush()?;

        let elapsed = start_time.elapsed();
        let average_speed = if elapsed.as_secs_f64() > 0.0 {
            (bytes_written as f64 / elapsed.as_secs_f64()) as u64
        } else {
            0
        };

        Ok(PassResult {
            pass_number,
            bytes_written,
            block_size: self.config.block_size,
            elapsed,
            average_speed_bps: average_speed,
            min_speed_bps: speed_tracker.min_speed(),
            max_speed_bps: speed_tracker.max_speed(),
        })
    }

    fn calculate_summary(&self, passes: &[PassResult]) -> BenchmarkSummary {
        let total_bytes: u64 = passes.iter().map(|p| p.bytes_written).sum();
        let total_elapsed: Duration = passes.iter().map(|p| p.elapsed).sum();

        let min_speed = passes.iter().map(|p| p.min_speed_bps).min().unwrap_or(0);
        let max_speed = passes.iter().map(|p| p.max_speed_bps).max().unwrap_or(0);

        let average_speed = if total_elapsed.as_secs_f64() > 0.0 {
            (total_bytes as f64 / total_elapsed.as_secs_f64()) as u64
        } else {
            0
        };

        BenchmarkSummary {
            total_bytes_written: total_bytes,
            total_elapsed,
            average_speed_bps: average_speed,
            min_speed_bps: min_speed,
            max_speed_bps: max_speed,
        }
    }

    /// Run benchmarks at multiple block sizes
    pub fn run_multi_block_sizes<W, F>(
        config: BenchmarkConfig,
        block_sizes: &[u64],
        mut target: W,
        device_path: &str,
        progress_callback: Option<F>,
        cancel_flag: Arc<AtomicBool>,
    ) -> Result<Vec<BlockSizeTestResult>>
    where
        W: Write + Seek,
        F: Fn(&BenchmarkProgress, usize, usize), // progress, current_test, total_tests
    {
        let effective_size =
            BenchmarkConfig::effective_test_size_for_block_sizes(config.test_size, block_sizes);

        let mut results = Vec::with_capacity(block_sizes.len());

        for (idx, &block_size) in block_sizes.iter().enumerate() {
            if cancel_flag.load(Ordering::Relaxed) {
                return Err(BenchmarkError::Cancelled);
            }

            let test_config = BenchmarkConfig {
                test_size: effective_size,
                block_size,
                pattern: config.pattern,
                passes: 1, // Single pass per block size
            };

            let runner = BenchmarkRunner {
                config: test_config,
                cancel_flag: Arc::clone(&cancel_flag),
            };

            // Wrap the progress callback to include test index
            let wrapped_callback = progress_callback.as_ref().map(|cb| {
                move |progress: &BenchmarkProgress| {
                    cb(progress, idx + 1, block_sizes.len());
                }
            });

            let result = runner.run(&mut target, device_path, wrapped_callback)?;

            results.push(BlockSizeTestResult {
                block_size,
                block_size_display: format_size(block_size),
                average_speed_bps: result.summary.average_speed_bps,
                speed_display: format_speed(result.summary.average_speed_bps),
            });
        }

        Ok(results)
    }
}

// Helper functions

/// Check if a value is a power of 2
pub fn is_power_of_two(n: u64) -> bool {
    n > 0 && (n & (n - 1)) == 0
}

/// Parse a size string like "256M", "1G", "4K" into bytes
pub fn parse_size(s: &str) -> Result<u64> {
    let s = s.trim();
    if s.is_empty() {
        return Err(BenchmarkError::InvalidSizeFormat(
            "empty string".to_string(),
        ));
    }

    let (num_str, suffix) = if s.chars().last().is_some_and(|c| c.is_alphabetic()) {
        let split_pos = s.chars().position(|c| c.is_alphabetic()).unwrap_or(s.len());
        (&s[..split_pos], &s[split_pos..])
    } else {
        (s, "")
    };

    let num: u64 = num_str
        .parse()
        .map_err(|_| BenchmarkError::InvalidSizeFormat(s.to_string()))?;

    let multiplier = match suffix.to_uppercase().as_str() {
        "" | "B" => 1,
        "K" | "KB" => 1024,
        "M" | "MB" => 1024 * 1024,
        "G" | "GB" => 1024 * 1024 * 1024,
        _ => return Err(BenchmarkError::InvalidSizeFormat(s.to_string())),
    };

    let result = num
        .checked_mul(multiplier)
        .ok_or_else(|| BenchmarkError::InvalidSizeFormat(format!("{} is too large", s)))?;

    // Validate power of 2
    if !is_power_of_two(result) {
        return Err(BenchmarkError::NotPowerOfTwo(s.to_string()));
    }

    Ok(result)
}

/// Parse a comma-separated list of block sizes
pub fn parse_block_sizes(s: &str) -> Result<Vec<u64>> {
    let sizes: Result<Vec<u64>> = s
        .split(',')
        .map(|part| {
            let size = parse_size(part.trim())?;
            if size > MAX_BLOCK_SIZE {
                return Err(BenchmarkError::BlockSizeTooLarge(part.trim().to_string()));
            }
            if size < MIN_BLOCK_SIZE {
                return Err(BenchmarkError::BlockSizeTooSmall(part.trim().to_string()));
            }
            Ok(size)
        })
        .collect();

    sizes
}

/// Format bytes as human-readable size
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{} GB", bytes / GB)
    } else if bytes >= MB {
        format!("{} MB", bytes / MB)
    } else if bytes >= KB {
        format!("{} KB", bytes / KB)
    } else {
        format!("{} B", bytes)
    }
}

/// Format speed as human-readable string
pub fn format_speed(bytes_per_sec: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes_per_sec >= GB {
        format!("{:.1} GB/s", bytes_per_sec as f64 / GB as f64)
    } else if bytes_per_sec >= MB {
        format!("{:.1} MB/s", bytes_per_sec as f64 / MB as f64)
    } else if bytes_per_sec >= KB {
        format!("{:.1} KB/s", bytes_per_sec as f64 / KB as f64)
    } else {
        format!("{} B/s", bytes_per_sec)
    }
}

/// Format duration as human-readable string
pub fn format_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs >= 60 {
        let mins = secs / 60;
        let remaining_secs = secs % 60;
        format!("{}m {}s", mins, remaining_secs)
    } else {
        format!("{:.2}s", duration.as_secs_f64())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::str::FromStr;

    #[test]
    fn test_is_power_of_two() {
        assert!(is_power_of_two(1));
        assert!(is_power_of_two(2));
        assert!(is_power_of_two(4));
        assert!(is_power_of_two(1024));
        assert!(is_power_of_two(1024 * 1024));

        assert!(!is_power_of_two(0));
        assert!(!is_power_of_two(3));
        assert!(!is_power_of_two(5));
        assert!(!is_power_of_two(100));
    }

    #[test]
    fn test_parse_size() {
        assert_eq!(parse_size("4K").unwrap(), 4 * 1024);
        assert_eq!(parse_size("64K").unwrap(), 64 * 1024);
        assert_eq!(parse_size("1M").unwrap(), 1024 * 1024);
        assert_eq!(parse_size("4M").unwrap(), 4 * 1024 * 1024);
        assert_eq!(parse_size("256M").unwrap(), 256 * 1024 * 1024);
        assert_eq!(parse_size("1G").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_size("1024").unwrap(), 1024);

        // Case insensitive
        assert_eq!(parse_size("4k").unwrap(), 4 * 1024);
        assert_eq!(parse_size("4KB").unwrap(), 4 * 1024);
        assert_eq!(parse_size("1mb").unwrap(), 1024 * 1024);

        // Not power of 2
        assert!(parse_size("100M").is_err());
        assert!(parse_size("3K").is_err());
    }

    #[test]
    fn test_parse_block_sizes() {
        let sizes = parse_block_sizes("4K,64K,1M,4M").unwrap();
        assert_eq!(sizes.len(), 4);
        assert_eq!(sizes[0], 4 * 1024);
        assert_eq!(sizes[1], 64 * 1024);
        assert_eq!(sizes[2], 1024 * 1024);
        assert_eq!(sizes[3], 4 * 1024 * 1024);

        // With spaces
        let sizes = parse_block_sizes("4K, 1M, 16M").unwrap();
        assert_eq!(sizes.len(), 3);

        // Too large
        assert!(parse_block_sizes("128M").is_err());
    }

    #[test]
    fn test_effective_test_size() {
        // With small block size, use requested size
        let config = BenchmarkConfig::new(256 * 1024 * 1024, 4 * 1024 * 1024);
        assert_eq!(config.effective_test_size(), 256 * 1024 * 1024);

        // With large block size, scale up
        let config = BenchmarkConfig::new(256 * 1024 * 1024, 64 * 1024 * 1024);
        assert_eq!(config.effective_test_size(), 640 * 1024 * 1024); // 64M * 10
    }

    #[test]
    fn test_effective_test_size_for_block_sizes() {
        let block_sizes = vec![4 * 1024, 1024 * 1024, 64 * 1024 * 1024];
        let effective =
            BenchmarkConfig::effective_test_size_for_block_sizes(256 * 1024 * 1024, &block_sizes);
        assert_eq!(effective, 640 * 1024 * 1024); // 64M * 10
    }

    #[test]
    fn test_benchmark_config_validate() {
        let config = BenchmarkConfig::new(256 * 1024 * 1024, 4 * 1024 * 1024);
        assert!(config.validate().is_ok());

        // Invalid: not power of 2
        let config = BenchmarkConfig::new(100 * 1024 * 1024, 4 * 1024 * 1024);
        assert!(config.validate().is_err());

        // Invalid: block size too large
        let config = BenchmarkConfig::new(256 * 1024 * 1024, 128 * 1024 * 1024);
        assert!(config.validate().is_err());

        // Invalid: block size too small
        let config = BenchmarkConfig::new(256 * 1024 * 1024, 1024);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_benchmark_runner_simple() {
        let config = BenchmarkConfig {
            test_size: 64 * 1024, // 64 KB
            block_size: 4 * 1024, // 4 KB
            pattern: DataPattern::Zeros,
            passes: 1,
        };

        let runner = BenchmarkRunner::new(config);
        let buffer = vec![0u8; 128 * 1024]; // 128 KB buffer
        let cursor = Cursor::new(buffer);

        let result = runner
            .run(cursor, "/dev/test", None::<fn(&BenchmarkProgress)>)
            .unwrap();

        assert_eq!(result.passes.len(), 1);
        assert_eq!(result.passes[0].bytes_written, 64 * 1024);
        assert!(result.summary.average_speed_bps > 0);
    }

    #[test]
    fn test_data_pattern_from_str() {
        assert_eq!(DataPattern::from_str("zeros").unwrap(), DataPattern::Zeros);
        assert_eq!(DataPattern::from_str("ZEROS").unwrap(), DataPattern::Zeros);
        assert_eq!(
            DataPattern::from_str("random").unwrap(),
            DataPattern::Random
        );
        assert_eq!(
            DataPattern::from_str("sequential").unwrap(),
            DataPattern::Sequential
        );
        assert_eq!(
            DataPattern::from_str("seq").unwrap(),
            DataPattern::Sequential
        );
        assert!(DataPattern::from_str("invalid").is_err());
    }

    #[test]
    fn test_format_speed() {
        assert_eq!(format_speed(500), "500 B/s");
        assert_eq!(format_speed(1024), "1.0 KB/s");
        assert_eq!(format_speed(1024 * 1024), "1.0 MB/s");
        assert_eq!(format_speed(50 * 1024 * 1024), "50.0 MB/s");
        assert_eq!(format_speed(1024 * 1024 * 1024), "1.0 GB/s");
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(Duration::from_secs_f64(0.5)), "0.50s");
        assert_eq!(format_duration(Duration::from_secs(30)), "30.00s");
        assert_eq!(format_duration(Duration::from_secs(90)), "1m 30s");
        assert_eq!(format_duration(Duration::from_secs(125)), "2m 5s");
    }

    #[test]
    fn test_progress_percentage() {
        let progress = BenchmarkProgress {
            bytes_written: 50,
            total_bytes: 100,
            current_pass: 1,
            total_passes: 1,
            current_speed_bps: 1000,
            elapsed: Duration::from_secs(1),
        };
        assert_eq!(progress.percentage(), 50);

        let progress = BenchmarkProgress {
            bytes_written: 100,
            total_bytes: 100,
            current_pass: 1,
            total_passes: 1,
            current_speed_bps: 1000,
            elapsed: Duration::from_secs(1),
        };
        assert_eq!(progress.percentage(), 100);
    }
}
