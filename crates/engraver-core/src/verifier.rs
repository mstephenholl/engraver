//! Verification and checksum module for Engraver
//!
//! This module provides:
//! - Checksum calculation (SHA-256, SHA-512, MD5, CRC32)
//! - Post-write verification (read-back and compare)
//! - Checksum file parsing (.sha256, .md5, etc.)
//! - Progress tracking during verification
//!
//! ## Example
//!
//! ```no_run
//! use engraver_core::verifier::{Verifier, ChecksumAlgorithm};
//! use std::fs::File;
//!
//! // Calculate checksum
//! let mut file = File::open("image.iso")?;
//! let mut verifier = Verifier::new()
//!     .on_progress(|p| println!("{:.1}%", p.percentage()));
//!
//! let checksum = verifier.calculate_checksum(&mut file, ChecksumAlgorithm::Sha256, None)?;
//! println!("SHA-256: {}", checksum);
//!
//! // Verify against expected
//! let expected = "abc123...";
//! verifier.verify_checksum(&mut file, ChecksumAlgorithm::Sha256, expected, None)?;
//! # Ok::<(), engraver_core::Error>(())
//! ```

use crate::error::{Error, Result};
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

// ============================================================================
// Constants
// ============================================================================

/// Default block size for verification (1 MB)
pub const DEFAULT_VERIFY_BLOCK_SIZE: usize = 1024 * 1024;

/// Minimum block size (4 KB)
pub const MIN_VERIFY_BLOCK_SIZE: usize = 4 * 1024;

/// Maximum block size (16 MB)
pub const MAX_VERIFY_BLOCK_SIZE: usize = 16 * 1024 * 1024;

// ============================================================================
// Checksum Algorithm
// ============================================================================

/// Supported checksum algorithms
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ChecksumAlgorithm {
    /// SHA-256 (recommended)
    Sha256,
    /// SHA-512
    Sha512,
    /// MD5 (legacy, not recommended for security)
    Md5,
    /// CRC32 (fast, not cryptographic)
    Crc32,
}

impl ChecksumAlgorithm {
    /// Get the expected output length in bytes
    pub fn byte_length(&self) -> usize {
        match self {
            ChecksumAlgorithm::Sha256 => 32,
            ChecksumAlgorithm::Sha512 => 64,
            ChecksumAlgorithm::Md5 => 16,
            ChecksumAlgorithm::Crc32 => 4,
        }
    }

    /// Get the expected output length in hex characters
    pub fn hex_length(&self) -> usize {
        self.byte_length() * 2
    }

    /// Get algorithm name
    pub fn name(&self) -> &'static str {
        match self {
            ChecksumAlgorithm::Sha256 => "SHA-256",
            ChecksumAlgorithm::Sha512 => "SHA-512",
            ChecksumAlgorithm::Md5 => "MD5",
            ChecksumAlgorithm::Crc32 => "CRC32",
        }
    }

    /// Get common file extension for this algorithm
    pub fn extension(&self) -> &'static str {
        match self {
            ChecksumAlgorithm::Sha256 => ".sha256",
            ChecksumAlgorithm::Sha512 => ".sha512",
            ChecksumAlgorithm::Md5 => ".md5",
            ChecksumAlgorithm::Crc32 => ".crc32",
        }
    }

    /// Try to detect algorithm from a hex string length
    pub fn from_hex_length(len: usize) -> Option<Self> {
        match len {
            64 => Some(ChecksumAlgorithm::Sha256),
            128 => Some(ChecksumAlgorithm::Sha512),
            32 => Some(ChecksumAlgorithm::Md5),
            8 => Some(ChecksumAlgorithm::Crc32),
            _ => None,
        }
    }

    /// Try to detect algorithm from file extension
    pub fn from_extension(ext: &str) -> Option<Self> {
        let ext = ext.to_lowercase();
        match ext.as_str() {
            ".sha256" | "sha256" | ".sha256sum" => Some(ChecksumAlgorithm::Sha256),
            ".sha512" | "sha512" | ".sha512sum" => Some(ChecksumAlgorithm::Sha512),
            ".md5" | "md5" | ".md5sum" => Some(ChecksumAlgorithm::Md5),
            ".crc32" | "crc32" | ".crc" => Some(ChecksumAlgorithm::Crc32),
            _ => None,
        }
    }

    /// List all supported algorithms
    pub fn all() -> &'static [ChecksumAlgorithm] {
        &[
            ChecksumAlgorithm::Sha256,
            ChecksumAlgorithm::Sha512,
            ChecksumAlgorithm::Md5,
            ChecksumAlgorithm::Crc32,
        ]
    }
}

impl std::fmt::Display for ChecksumAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

impl std::str::FromStr for ChecksumAlgorithm {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        let s = s.to_lowercase();
        match s.as_str() {
            "sha256" | "sha-256" => Ok(ChecksumAlgorithm::Sha256),
            "sha512" | "sha-512" => Ok(ChecksumAlgorithm::Sha512),
            "md5" => Ok(ChecksumAlgorithm::Md5),
            "crc32" | "crc-32" => Ok(ChecksumAlgorithm::Crc32),
            _ => Err(Error::InvalidConfig(format!(
                "Unknown checksum algorithm: {}",
                s
            ))),
        }
    }
}

// ============================================================================
// Checksum Result
// ============================================================================

/// Checksum calculation result
#[derive(Debug, Clone)]
pub struct Checksum {
    /// The algorithm used
    pub algorithm: ChecksumAlgorithm,
    /// The checksum bytes
    pub bytes: Vec<u8>,
}

impl Checksum {
    /// Create a new checksum from bytes
    pub fn new(algorithm: ChecksumAlgorithm, bytes: Vec<u8>) -> Self {
        Self { algorithm, bytes }
    }

    /// Create a checksum from a hex string
    pub fn from_hex(algorithm: ChecksumAlgorithm, hex: &str) -> Result<Self> {
        let hex = hex.trim().to_lowercase();

        if hex.len() != algorithm.hex_length() {
            return Err(Error::InvalidConfig(format!(
                "Invalid {} checksum length: expected {}, got {}",
                algorithm.name(),
                algorithm.hex_length(),
                hex.len()
            )));
        }

        let bytes = hex_to_bytes(&hex)?;
        Ok(Self { algorithm, bytes })
    }

    /// Get the checksum as a lowercase hex string
    pub fn to_hex(&self) -> String {
        bytes_to_hex(&self.bytes)
    }

    /// Check if this checksum matches another
    pub fn matches(&self, other: &Checksum) -> bool {
        self.algorithm == other.algorithm && self.bytes == other.bytes
    }

    /// Check if this checksum matches a hex string
    pub fn matches_hex(&self, hex: &str) -> bool {
        let hex = hex.trim().to_lowercase();
        self.to_hex() == hex
    }
}

impl std::fmt::Display for Checksum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

impl PartialEq for Checksum {
    fn eq(&self, other: &Self) -> bool {
        self.matches(other)
    }
}

// ============================================================================
// Verification Progress
// ============================================================================

/// Progress callback type
pub type ProgressCallback = Box<dyn FnMut(&VerificationProgress) + Send>;

/// Verification progress information
#[derive(Debug, Clone)]
pub struct VerificationProgress {
    /// Bytes processed so far
    pub bytes_processed: u64,
    /// Total bytes to process (if known)
    pub total_bytes: Option<u64>,
    /// Current speed in bytes per second
    pub speed_bps: u64,
    /// Estimated time remaining
    pub eta_seconds: Option<u64>,
    /// Elapsed time
    pub elapsed: Duration,
    /// Current operation
    pub operation: VerificationOperation,
}

impl VerificationProgress {
    /// Calculate completion percentage (0-100)
    pub fn percentage(&self) -> f64 {
        match self.total_bytes {
            Some(total) if total > 0 => (self.bytes_processed as f64 / total as f64) * 100.0,
            Some(_) => 100.0, // total is 0
            None => 0.0,      // unknown total
        }
    }

    /// Format speed for display
    pub fn speed_display(&self) -> String {
        crate::format_speed(self.speed_bps)
    }

    /// Format ETA for display
    pub fn eta_display(&self) -> String {
        match self.eta_seconds {
            Some(secs) => crate::format_duration(secs),
            None => "unknown".to_string(),
        }
    }
}

/// Type of verification operation in progress
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerificationOperation {
    /// Calculating checksum
    Checksum,
    /// Comparing source and target
    Compare,
    /// Reading source
    ReadSource,
    /// Reading target
    ReadTarget,
}

impl std::fmt::Display for VerificationOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerificationOperation::Checksum => write!(f, "Calculating checksum"),
            VerificationOperation::Compare => write!(f, "Comparing"),
            VerificationOperation::ReadSource => write!(f, "Reading source"),
            VerificationOperation::ReadTarget => write!(f, "Reading target"),
        }
    }
}

// ============================================================================
// Verification Result
// ============================================================================

/// Result of a verification operation
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// Whether verification passed
    pub success: bool,
    /// Bytes verified
    pub bytes_verified: u64,
    /// Number of mismatches found (for comparison)
    pub mismatches: u64,
    /// First mismatch offset (if any)
    pub first_mismatch_offset: Option<u64>,
    /// Elapsed time
    pub elapsed: Duration,
    /// Average speed
    pub speed_bps: u64,
}

impl VerificationResult {
    /// Create a successful result
    pub fn success(bytes_verified: u64, elapsed: Duration) -> Self {
        let speed_bps = if elapsed.as_secs_f64() > 0.0 {
            (bytes_verified as f64 / elapsed.as_secs_f64()) as u64
        } else {
            0
        };

        Self {
            success: true,
            bytes_verified,
            mismatches: 0,
            first_mismatch_offset: None,
            elapsed,
            speed_bps,
        }
    }

    /// Create a failed result
    pub fn failure(
        bytes_verified: u64,
        mismatches: u64,
        first_mismatch_offset: Option<u64>,
        elapsed: Duration,
    ) -> Self {
        let speed_bps = if elapsed.as_secs_f64() > 0.0 {
            (bytes_verified as f64 / elapsed.as_secs_f64()) as u64
        } else {
            0
        };

        Self {
            success: false,
            bytes_verified,
            mismatches,
            first_mismatch_offset,
            elapsed,
            speed_bps,
        }
    }
}

// ============================================================================
// Verifier
// ============================================================================

/// Verification configuration
#[derive(Debug, Clone)]
pub struct VerifyConfig {
    /// Block size for reading
    pub block_size: usize,
    /// Stop on first mismatch
    pub stop_on_mismatch: bool,
}

impl Default for VerifyConfig {
    fn default() -> Self {
        Self {
            block_size: DEFAULT_VERIFY_BLOCK_SIZE,
            stop_on_mismatch: true,
        }
    }
}

impl VerifyConfig {
    /// Create a new config with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set block size (clamped to valid range)
    pub fn block_size(mut self, size: usize) -> Self {
        self.block_size = size.clamp(MIN_VERIFY_BLOCK_SIZE, MAX_VERIFY_BLOCK_SIZE);
        self
    }

    /// Set whether to stop on first mismatch
    pub fn stop_on_mismatch(mut self, stop: bool) -> Self {
        self.stop_on_mismatch = stop;
        self
    }
}

/// Verifier for checksums and data comparison
pub struct Verifier {
    config: VerifyConfig,
    progress_callback: Option<ProgressCallback>,
    cancel_flag: Arc<AtomicBool>,
}

impl Verifier {
    /// Create a new Verifier with default configuration
    pub fn new() -> Self {
        Self {
            config: VerifyConfig::default(),
            progress_callback: None,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Create a Verifier with custom configuration
    pub fn with_config(config: VerifyConfig) -> Self {
        Self {
            config,
            progress_callback: None,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Set progress callback
    pub fn on_progress<F>(mut self, callback: F) -> Self
    where
        F: FnMut(&VerificationProgress) + Send + 'static,
    {
        self.progress_callback = Some(Box::new(callback));
        self
    }

    /// Get a handle to cancel the operation
    pub fn cancel_handle(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel_flag)
    }

    /// Calculate checksum of a reader
    #[cfg(feature = "checksum")]
    pub fn calculate_checksum<R: Read + ?Sized>(
        &mut self,
        reader: &mut R,
        algorithm: ChecksumAlgorithm,
        total_size: Option<u64>,
    ) -> Result<Checksum> {
        use sha2::Digest;

        self.cancel_flag.store(false, Ordering::SeqCst);
        let start = Instant::now();
        let mut bytes_processed = 0u64;
        let mut buffer = vec![0u8; self.config.block_size];

        // Create the appropriate hasher
        enum Hasher {
            Sha256(sha2::Sha256),
            Sha512(sha2::Sha512),
            Md5(md5::Md5),
            Crc32(crc32fast::Hasher),
        }

        let mut hasher = match algorithm {
            ChecksumAlgorithm::Sha256 => Hasher::Sha256(sha2::Sha256::new()),
            ChecksumAlgorithm::Sha512 => Hasher::Sha512(sha2::Sha512::new()),
            ChecksumAlgorithm::Md5 => Hasher::Md5(md5::Md5::new()),
            ChecksumAlgorithm::Crc32 => Hasher::Crc32(crc32fast::Hasher::new()),
        };

        loop {
            // Check for cancellation
            if self.cancel_flag.load(Ordering::SeqCst) {
                return Err(Error::Cancelled);
            }

            let n = read_full(reader, &mut buffer)?;
            if n == 0 {
                break;
            }

            // Update hasher
            match &mut hasher {
                Hasher::Sha256(h) => h.update(&buffer[..n]),
                Hasher::Sha512(h) => h.update(&buffer[..n]),
                Hasher::Md5(h) => h.update(&buffer[..n]),
                Hasher::Crc32(h) => h.update(&buffer[..n]),
            }

            bytes_processed += n as u64;

            // Report progress
            if let Some(ref mut callback) = self.progress_callback {
                let elapsed = start.elapsed();
                let speed_bps = if elapsed.as_secs_f64() > 0.0 {
                    (bytes_processed as f64 / elapsed.as_secs_f64()) as u64
                } else {
                    0
                };

                let eta_seconds = total_size.and_then(|total| {
                    if speed_bps > 0 && bytes_processed < total {
                        Some((total - bytes_processed) / speed_bps)
                    } else {
                        None
                    }
                });

                callback(&VerificationProgress {
                    bytes_processed,
                    total_bytes: total_size,
                    speed_bps,
                    eta_seconds,
                    elapsed,
                    operation: VerificationOperation::Checksum,
                });
            }
        }

        // Finalize and get result
        let bytes = match hasher {
            Hasher::Sha256(h) => h.finalize().to_vec(),
            Hasher::Sha512(h) => h.finalize().to_vec(),
            Hasher::Md5(h) => h.finalize().to_vec(),
            Hasher::Crc32(h) => h.finalize().to_be_bytes().to_vec(),
        };

        Ok(Checksum::new(algorithm, bytes))
    }

    /// Calculate checksum and verify against expected value
    #[cfg(feature = "checksum")]
    pub fn verify_checksum<R: Read + ?Sized>(
        &mut self,
        reader: &mut R,
        algorithm: ChecksumAlgorithm,
        expected: &str,
        total_size: Option<u64>,
    ) -> Result<VerificationResult> {
        let start = Instant::now();

        let actual = self.calculate_checksum(reader, algorithm, total_size)?;
        let elapsed = start.elapsed();

        if actual.matches_hex(expected) {
            Ok(VerificationResult::success(
                total_size.unwrap_or(0),
                elapsed,
            ))
        } else {
            Err(Error::ChecksumMismatch {
                expected: expected.to_lowercase(),
                actual: actual.to_hex(),
            })
        }
    }

    /// Compare source and target byte-by-byte
    pub fn compare<R, T>(
        &mut self,
        source: &mut R,
        target: &mut T,
        size: u64,
    ) -> Result<VerificationResult>
    where
        R: Read + Seek + ?Sized,
        T: Read + Seek + ?Sized,
    {
        self.cancel_flag.store(false, Ordering::SeqCst);
        let start = Instant::now();

        // Seek both to start
        source.seek(SeekFrom::Start(0))?;
        target.seek(SeekFrom::Start(0))?;

        let block_size = self.config.block_size;
        let mut source_buf = vec![0u8; block_size];
        let mut target_buf = vec![0u8; block_size];
        let mut bytes_verified = 0u64;
        let mut mismatches = 0u64;
        let mut first_mismatch: Option<u64> = None;

        while bytes_verified < size {
            // Check for cancellation
            if self.cancel_flag.load(Ordering::SeqCst) {
                return Err(Error::Cancelled);
            }

            let to_read = block_size.min((size - bytes_verified) as usize);

            let source_read = read_full(source, &mut source_buf[..to_read])?;
            let target_read = read_full(target, &mut target_buf[..to_read])?;

            // Check for size mismatch
            if source_read != target_read {
                mismatches += 1;
                if first_mismatch.is_none() {
                    first_mismatch = Some(bytes_verified);
                }
                if self.config.stop_on_mismatch {
                    let elapsed = start.elapsed();
                    return Ok(VerificationResult::failure(
                        bytes_verified,
                        mismatches,
                        first_mismatch,
                        elapsed,
                    ));
                }
            } else if source_buf[..source_read] != target_buf[..target_read] {
                mismatches += 1;
                if first_mismatch.is_none() {
                    // Find exact offset
                    for i in 0..source_read {
                        if source_buf[i] != target_buf[i] {
                            first_mismatch = Some(bytes_verified + i as u64);
                            break;
                        }
                    }
                }
                if self.config.stop_on_mismatch {
                    let elapsed = start.elapsed();
                    return Ok(VerificationResult::failure(
                        bytes_verified,
                        mismatches,
                        first_mismatch,
                        elapsed,
                    ));
                }
            }

            bytes_verified += source_read as u64;

            // Report progress
            if let Some(ref mut callback) = self.progress_callback {
                let elapsed = start.elapsed();
                let speed_bps = if elapsed.as_secs_f64() > 0.0 {
                    (bytes_verified as f64 / elapsed.as_secs_f64()) as u64
                } else {
                    0
                };

                let eta_seconds = if speed_bps > 0 && bytes_verified < size {
                    Some((size - bytes_verified) / speed_bps)
                } else {
                    None
                };

                callback(&VerificationProgress {
                    bytes_processed: bytes_verified,
                    total_bytes: Some(size),
                    speed_bps,
                    eta_seconds,
                    elapsed,
                    operation: VerificationOperation::Compare,
                });
            }

            if source_read < to_read {
                break; // EOF
            }
        }

        let elapsed = start.elapsed();
        if mismatches == 0 {
            Ok(VerificationResult::success(bytes_verified, elapsed))
        } else {
            Ok(VerificationResult::failure(
                bytes_verified,
                mismatches,
                first_mismatch,
                elapsed,
            ))
        }
    }
}

impl Default for Verifier {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// Checksum File Parsing
// ============================================================================

/// Parsed checksum entry from a checksum file
#[derive(Debug, Clone)]
pub struct ChecksumEntry {
    /// The checksum value
    pub checksum: String,
    /// The filename
    pub filename: String,
    /// The algorithm (if detected)
    pub algorithm: Option<ChecksumAlgorithm>,
}

/// Parse a checksum file (e.g., .sha256, .md5)
///
/// Supports formats:
/// - `checksum  filename` (GNU coreutils format)
/// - `checksum *filename` (binary mode indicator)
/// - `ALGORITHM (filename) = checksum` (BSD format)
pub fn parse_checksum_file(content: &str) -> Vec<ChecksumEntry> {
    let mut entries = Vec::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Try BSD format: ALGORITHM (filename) = checksum
        if let Some(entry) = parse_bsd_format(line) {
            entries.push(entry);
            continue;
        }

        // Try GNU format: checksum  filename or checksum *filename
        if let Some(entry) = parse_gnu_format(line) {
            entries.push(entry);
            continue;
        }
    }

    entries
}

fn parse_bsd_format(line: &str) -> Option<ChecksumEntry> {
    // Format: ALGORITHM (filename) = checksum
    let parts: Vec<&str> = line.splitn(2, " (").collect();
    if parts.len() != 2 {
        return None;
    }

    let algorithm = parts[0].parse::<ChecksumAlgorithm>().ok();

    let rest = parts[1];
    let parts: Vec<&str> = rest.splitn(2, ") = ").collect();
    if parts.len() != 2 {
        return None;
    }

    let filename = parts[0].to_string();
    let checksum = parts[1].trim().to_lowercase();

    Some(ChecksumEntry {
        checksum,
        filename,
        algorithm,
    })
}

fn parse_gnu_format(line: &str) -> Option<ChecksumEntry> {
    // Format: checksum  filename or checksum *filename
    // Find the first space followed by a space or asterisk
    let mut split_idx = None;
    let chars: Vec<char> = line.chars().collect();

    for i in 0..chars.len() {
        if chars[i] == ' ' && i + 1 < chars.len() && (chars[i + 1] == ' ' || chars[i + 1] == '*') {
            split_idx = Some(i);
            break;
        }
    }

    let idx = split_idx?;
    let checksum = line[..idx].trim().to_lowercase();
    let mut filename = line[idx..].trim();

    // Remove leading asterisk (binary mode indicator)
    if filename.starts_with('*') {
        filename = &filename[1..];
    }

    // Detect algorithm from checksum length
    let algorithm = ChecksumAlgorithm::from_hex_length(checksum.len());

    Some(ChecksumEntry {
        checksum,
        filename: filename.to_string(),
        algorithm,
    })
}

/// Result of auto-detecting a checksum file
#[derive(Debug, Clone)]
pub struct DetectedChecksum {
    /// The checksum value (hex string)
    pub checksum: String,
    /// The detected algorithm
    pub algorithm: ChecksumAlgorithm,
    /// Path to the checksum file that was found
    pub source_file: std::path::PathBuf,
}

/// Attempt to find and parse a checksum file for the given source path
///
/// This function looks for checksum files in common locations:
/// 1. `{source}.sha256`, `{source}.sha512`, `{source}.md5` (direct extensions)
/// 2. `{source}.sha256sum`, `{source}.sha512sum`, `{source}.md5sum`
/// 3. `SHA256SUMS`, `SHA512SUMS`, `MD5SUMS` in the same directory
///
/// Returns the checksum value and algorithm if found.
///
/// # Example
///
/// ```no_run
/// use engraver_core::verifier::auto_detect_checksum;
///
/// // If ubuntu.iso.sha256 exists alongside ubuntu.iso
/// if let Some(detected) = auto_detect_checksum("ubuntu.iso") {
///     println!("Found {} checksum: {}", detected.algorithm, detected.checksum);
/// }
/// ```
pub fn auto_detect_checksum(source_path: &str) -> Option<DetectedChecksum> {
    use std::path::Path;

    let source = Path::new(source_path);

    // Skip auto-detection for URLs
    if source_path.starts_with("http://") || source_path.starts_with("https://") {
        return None;
    }

    // Get the source filename for matching in SUMS files
    let source_filename = source.file_name()?.to_str()?;
    let parent_dir = source.parent().unwrap_or_else(|| Path::new("."));

    // Extensions to try (in order of preference)
    let direct_extensions = [
        ("sha256", ChecksumAlgorithm::Sha256),
        ("sha256sum", ChecksumAlgorithm::Sha256),
        ("sha512", ChecksumAlgorithm::Sha512),
        ("sha512sum", ChecksumAlgorithm::Sha512),
        ("md5", ChecksumAlgorithm::Md5),
        ("md5sum", ChecksumAlgorithm::Md5),
    ];

    // Try direct extensions: source.sha256, source.sha256sum, etc.
    for (ext, algorithm) in &direct_extensions {
        let checksum_path = source.with_extension(
            source
                .extension()
                .map(|e| format!("{}.{}", e.to_string_lossy(), ext))
                .unwrap_or_else(|| ext.to_string()),
        );

        if let Some(detected) = try_parse_checksum_file(&checksum_path, source_filename, *algorithm)
        {
            return Some(detected);
        }
    }

    // SUMS files to check in the same directory
    let sums_files = [
        ("SHA256SUMS", ChecksumAlgorithm::Sha256),
        ("SHA256SUM", ChecksumAlgorithm::Sha256),
        ("sha256sums", ChecksumAlgorithm::Sha256),
        ("sha256sum.txt", ChecksumAlgorithm::Sha256),
        ("SHA512SUMS", ChecksumAlgorithm::Sha512),
        ("SHA512SUM", ChecksumAlgorithm::Sha512),
        ("sha512sums", ChecksumAlgorithm::Sha512),
        ("MD5SUMS", ChecksumAlgorithm::Md5),
        ("MD5SUM", ChecksumAlgorithm::Md5),
        ("md5sums", ChecksumAlgorithm::Md5),
        ("md5sum.txt", ChecksumAlgorithm::Md5),
    ];

    // Try SUMS files in the same directory
    for (sums_filename, algorithm) in &sums_files {
        let sums_path = parent_dir.join(sums_filename);
        if let Some(detected) = try_parse_checksum_file(&sums_path, source_filename, *algorithm) {
            return Some(detected);
        }
    }

    None
}

/// Try to parse a checksum file and find the entry for the given filename
fn try_parse_checksum_file(
    checksum_path: &std::path::Path,
    source_filename: &str,
    expected_algorithm: ChecksumAlgorithm,
) -> Option<DetectedChecksum> {
    if !checksum_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(checksum_path).ok()?;
    let entries = parse_checksum_file(&content);

    // Find the entry for our source file
    let entry = find_checksum_for_file(&entries, source_filename)?;

    // Verify the checksum looks valid
    let algorithm = entry.algorithm.unwrap_or(expected_algorithm);
    if entry.checksum.len() != algorithm.hex_length() {
        return None;
    }

    Some(DetectedChecksum {
        checksum: entry.checksum.clone(),
        algorithm,
        source_file: checksum_path.to_path_buf(),
    })
}

/// Find checksum for a specific filename in entries
pub fn find_checksum_for_file<'a>(
    entries: &'a [ChecksumEntry],
    filename: &str,
) -> Option<&'a ChecksumEntry> {
    // Try exact match first
    if let Some(entry) = entries.iter().find(|e| e.filename == filename) {
        return Some(entry);
    }

    // Try matching just the filename (without path)
    let base_name = std::path::Path::new(filename)
        .file_name()
        .and_then(|s| s.to_str())?;

    entries.iter().find(|e| {
        let entry_base = std::path::Path::new(&e.filename)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&e.filename);
        entry_base == base_name
    })
}

// ============================================================================
// Legacy API (for backwards compatibility)
// ============================================================================

/// Verify that target matches source by reading back (legacy function)
pub fn verify_write<R, T>(
    source: &mut R,
    target: &mut T,
    size: u64,
    block_size: usize,
) -> Result<VerificationResult>
where
    R: Read + Seek,
    T: Read + Seek,
{
    let config = VerifyConfig::new()
        .block_size(block_size)
        .stop_on_mismatch(false);

    let mut verifier = Verifier::with_config(config);
    verifier.compare(source, target, size)
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Read as much as possible into buffer
fn read_full<R: Read + ?Sized>(reader: &mut R, buf: &mut [u8]) -> Result<usize> {
    let mut total = 0;
    while total < buf.len() {
        match reader.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(Error::Io(e)),
        }
    }
    Ok(total)
}

/// Convert bytes to lowercase hex string
fn bytes_to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Convert hex string to bytes
fn hex_to_bytes(hex: &str) -> Result<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return Err(Error::InvalidConfig(
            "Hex string must have even length".to_string(),
        ));
    }

    (0..hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| {
                Error::InvalidConfig(format!("Invalid hex character at position {}", i))
            })
        })
        .collect()
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    // -------------------------------------------------------------------------
    // ChecksumAlgorithm tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_algorithm_byte_length() {
        assert_eq!(ChecksumAlgorithm::Sha256.byte_length(), 32);
        assert_eq!(ChecksumAlgorithm::Sha512.byte_length(), 64);
        assert_eq!(ChecksumAlgorithm::Md5.byte_length(), 16);
        assert_eq!(ChecksumAlgorithm::Crc32.byte_length(), 4);
    }

    #[test]
    fn test_algorithm_hex_length() {
        assert_eq!(ChecksumAlgorithm::Sha256.hex_length(), 64);
        assert_eq!(ChecksumAlgorithm::Sha512.hex_length(), 128);
        assert_eq!(ChecksumAlgorithm::Md5.hex_length(), 32);
        assert_eq!(ChecksumAlgorithm::Crc32.hex_length(), 8);
    }

    #[test]
    fn test_algorithm_name() {
        assert_eq!(ChecksumAlgorithm::Sha256.name(), "SHA-256");
        assert_eq!(ChecksumAlgorithm::Sha512.name(), "SHA-512");
        assert_eq!(ChecksumAlgorithm::Md5.name(), "MD5");
        assert_eq!(ChecksumAlgorithm::Crc32.name(), "CRC32");
    }

    #[test]
    fn test_algorithm_extension() {
        assert_eq!(ChecksumAlgorithm::Sha256.extension(), ".sha256");
        assert_eq!(ChecksumAlgorithm::Sha512.extension(), ".sha512");
        assert_eq!(ChecksumAlgorithm::Md5.extension(), ".md5");
        assert_eq!(ChecksumAlgorithm::Crc32.extension(), ".crc32");
    }

    #[test]
    fn test_algorithm_from_hex_length() {
        assert_eq!(
            ChecksumAlgorithm::from_hex_length(64),
            Some(ChecksumAlgorithm::Sha256)
        );
        assert_eq!(
            ChecksumAlgorithm::from_hex_length(128),
            Some(ChecksumAlgorithm::Sha512)
        );
        assert_eq!(
            ChecksumAlgorithm::from_hex_length(32),
            Some(ChecksumAlgorithm::Md5)
        );
        assert_eq!(
            ChecksumAlgorithm::from_hex_length(8),
            Some(ChecksumAlgorithm::Crc32)
        );
        assert_eq!(ChecksumAlgorithm::from_hex_length(100), None);
    }

    #[test]
    fn test_algorithm_from_extension() {
        assert_eq!(
            ChecksumAlgorithm::from_extension(".sha256"),
            Some(ChecksumAlgorithm::Sha256)
        );
        assert_eq!(
            ChecksumAlgorithm::from_extension("sha256"),
            Some(ChecksumAlgorithm::Sha256)
        );
        assert_eq!(
            ChecksumAlgorithm::from_extension(".md5"),
            Some(ChecksumAlgorithm::Md5)
        );
        assert_eq!(ChecksumAlgorithm::from_extension(".unknown"), None);
    }

    #[test]
    fn test_algorithm_from_str() {
        assert_eq!(
            "sha256".parse::<ChecksumAlgorithm>().unwrap(),
            ChecksumAlgorithm::Sha256
        );
        assert_eq!(
            "SHA-256".parse::<ChecksumAlgorithm>().unwrap(),
            ChecksumAlgorithm::Sha256
        );
        assert_eq!(
            "md5".parse::<ChecksumAlgorithm>().unwrap(),
            ChecksumAlgorithm::Md5
        );
        assert!("invalid".parse::<ChecksumAlgorithm>().is_err());
    }

    #[test]
    fn test_algorithm_display() {
        assert_eq!(format!("{}", ChecksumAlgorithm::Sha256), "SHA-256");
        assert_eq!(format!("{}", ChecksumAlgorithm::Md5), "MD5");
    }

    #[test]
    fn test_algorithm_all() {
        let all = ChecksumAlgorithm::all();
        assert_eq!(all.len(), 4);
        assert!(all.contains(&ChecksumAlgorithm::Sha256));
        assert!(all.contains(&ChecksumAlgorithm::Sha512));
        assert!(all.contains(&ChecksumAlgorithm::Md5));
        assert!(all.contains(&ChecksumAlgorithm::Crc32));
    }

    // -------------------------------------------------------------------------
    // Checksum tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_checksum_to_hex() {
        let checksum = Checksum::new(ChecksumAlgorithm::Md5, vec![0xab, 0xcd, 0xef]);
        assert_eq!(checksum.to_hex(), "abcdef");
    }

    #[test]
    fn test_checksum_from_hex() {
        let checksum = Checksum::from_hex(ChecksumAlgorithm::Crc32, "12345678").unwrap();
        assert_eq!(checksum.bytes, vec![0x12, 0x34, 0x56, 0x78]);
    }

    #[test]
    fn test_checksum_from_hex_invalid_length() {
        let result = Checksum::from_hex(ChecksumAlgorithm::Sha256, "1234");
        assert!(result.is_err());
    }

    #[test]
    fn test_checksum_matches() {
        let c1 = Checksum::new(ChecksumAlgorithm::Crc32, vec![1, 2, 3, 4]);
        let c2 = Checksum::new(ChecksumAlgorithm::Crc32, vec![1, 2, 3, 4]);
        let c3 = Checksum::new(ChecksumAlgorithm::Crc32, vec![1, 2, 3, 5]);

        assert!(c1.matches(&c2));
        assert!(!c1.matches(&c3));
    }

    #[test]
    fn test_checksum_matches_hex() {
        let checksum = Checksum::new(ChecksumAlgorithm::Crc32, vec![0xab, 0xcd, 0xef, 0x12]);
        assert!(checksum.matches_hex("abcdef12"));
        assert!(checksum.matches_hex("ABCDEF12"));
        assert!(!checksum.matches_hex("00000000"));
    }

    #[test]
    fn test_checksum_display() {
        let checksum = Checksum::new(ChecksumAlgorithm::Crc32, vec![0xab, 0xcd, 0xef, 0x12]);
        assert_eq!(format!("{}", checksum), "abcdef12");
    }

    // -------------------------------------------------------------------------
    // VerificationProgress tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_progress_percentage() {
        let progress = VerificationProgress {
            bytes_processed: 500,
            total_bytes: Some(1000),
            speed_bps: 100,
            eta_seconds: Some(5),
            elapsed: Duration::from_secs(5),
            operation: VerificationOperation::Checksum,
        };
        assert_eq!(progress.percentage(), 50.0);
    }

    #[test]
    fn test_progress_percentage_zero_total() {
        let progress = VerificationProgress {
            bytes_processed: 0,
            total_bytes: Some(0),
            speed_bps: 0,
            eta_seconds: None,
            elapsed: Duration::ZERO,
            operation: VerificationOperation::Checksum,
        };
        assert_eq!(progress.percentage(), 100.0);
    }

    #[test]
    fn test_progress_percentage_unknown_total() {
        let progress = VerificationProgress {
            bytes_processed: 500,
            total_bytes: None,
            speed_bps: 100,
            eta_seconds: None,
            elapsed: Duration::from_secs(5),
            operation: VerificationOperation::Checksum,
        };
        assert_eq!(progress.percentage(), 0.0);
    }

    #[test]
    fn test_operation_display() {
        assert_eq!(
            format!("{}", VerificationOperation::Checksum),
            "Calculating checksum"
        );
        assert_eq!(format!("{}", VerificationOperation::Compare), "Comparing");
    }

    // -------------------------------------------------------------------------
    // VerificationResult tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_result_success() {
        let result = VerificationResult::success(1000, Duration::from_secs(1));
        assert!(result.success);
        assert_eq!(result.bytes_verified, 1000);
        assert_eq!(result.mismatches, 0);
        assert!(result.first_mismatch_offset.is_none());
        assert_eq!(result.speed_bps, 1000);
    }

    #[test]
    fn test_result_failure() {
        let result = VerificationResult::failure(500, 2, Some(100), Duration::from_secs(1));
        assert!(!result.success);
        assert_eq!(result.bytes_verified, 500);
        assert_eq!(result.mismatches, 2);
        assert_eq!(result.first_mismatch_offset, Some(100));
    }

    // -------------------------------------------------------------------------
    // VerifyConfig tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_config_default() {
        let config = VerifyConfig::default();
        assert_eq!(config.block_size, DEFAULT_VERIFY_BLOCK_SIZE);
        assert!(config.stop_on_mismatch);
    }

    #[test]
    fn test_config_builder() {
        let config = VerifyConfig::new()
            .block_size(512 * 1024)
            .stop_on_mismatch(false);

        assert_eq!(config.block_size, 512 * 1024);
        assert!(!config.stop_on_mismatch);
    }

    #[test]
    fn test_config_block_size_clamping() {
        let config = VerifyConfig::new().block_size(100);
        assert_eq!(config.block_size, MIN_VERIFY_BLOCK_SIZE);

        let config = VerifyConfig::new().block_size(100 * 1024 * 1024);
        assert_eq!(config.block_size, MAX_VERIFY_BLOCK_SIZE);
    }

    // -------------------------------------------------------------------------
    // Verifier compare tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_compare_matching() {
        let data = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let mut source = Cursor::new(data.clone());
        let mut target = Cursor::new(data);

        let mut verifier = Verifier::new();
        let result = verifier.compare(&mut source, &mut target, 8).unwrap();

        assert!(result.success);
        assert_eq!(result.bytes_verified, 8);
        assert_eq!(result.mismatches, 0);
    }

    #[test]
    fn test_compare_mismatch() {
        let source_data = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let target_data = vec![1u8, 2, 3, 4, 5, 6, 7, 9];

        let mut source = Cursor::new(source_data);
        let mut target = Cursor::new(target_data);

        let config = VerifyConfig::new().stop_on_mismatch(true);
        let mut verifier = Verifier::with_config(config);
        let result = verifier.compare(&mut source, &mut target, 8).unwrap();

        assert!(!result.success);
        assert!(result.mismatches > 0);
        assert!(result.first_mismatch_offset.is_some());
    }

    #[test]
    fn test_compare_empty() {
        let mut source = Cursor::new(Vec::<u8>::new());
        let mut target = Cursor::new(Vec::<u8>::new());

        let mut verifier = Verifier::new();
        let result = verifier.compare(&mut source, &mut target, 0).unwrap();

        assert!(result.success);
        assert_eq!(result.bytes_verified, 0);
    }

    #[test]
    fn test_compare_with_progress() {
        let data = vec![0u8; MIN_VERIFY_BLOCK_SIZE * 4];
        let mut source = Cursor::new(data.clone());
        let mut target = Cursor::new(data.clone());

        let progress_count = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let progress_count_clone = Arc::clone(&progress_count);

        let config = VerifyConfig::new().block_size(MIN_VERIFY_BLOCK_SIZE);
        let mut verifier = Verifier::with_config(config).on_progress(move |_| {
            progress_count_clone.fetch_add(1, Ordering::SeqCst);
        });

        let result = verifier
            .compare(&mut source, &mut target, data.len() as u64)
            .unwrap();

        assert!(result.success);
        assert!(progress_count.load(Ordering::SeqCst) >= 4);
    }

    // -------------------------------------------------------------------------
    // Checksum calculation tests (require feature)
    // -------------------------------------------------------------------------

    #[cfg(feature = "checksum")]
    mod checksum_tests {
        use super::*;

        #[test]
        fn test_calculate_sha256() {
            // SHA-256 of empty string
            let mut reader = Cursor::new(Vec::<u8>::new());
            let mut verifier = Verifier::new();
            let checksum = verifier
                .calculate_checksum(&mut reader, ChecksumAlgorithm::Sha256, None)
                .unwrap();

            assert_eq!(
                checksum.to_hex(),
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
            );
        }

        #[test]
        fn test_calculate_sha256_hello() {
            // SHA-256 of "hello"
            let mut reader = Cursor::new(b"hello".to_vec());
            let mut verifier = Verifier::new();
            let checksum = verifier
                .calculate_checksum(&mut reader, ChecksumAlgorithm::Sha256, None)
                .unwrap();

            assert_eq!(
                checksum.to_hex(),
                "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
            );
        }

        #[test]
        fn test_calculate_md5() {
            // MD5 of "hello"
            let mut reader = Cursor::new(b"hello".to_vec());
            let mut verifier = Verifier::new();
            let checksum = verifier
                .calculate_checksum(&mut reader, ChecksumAlgorithm::Md5, None)
                .unwrap();

            assert_eq!(checksum.to_hex(), "5d41402abc4b2a76b9719d911017c592");
        }

        #[test]
        fn test_calculate_crc32() {
            // CRC32 of "hello"
            let mut reader = Cursor::new(b"hello".to_vec());
            let mut verifier = Verifier::new();
            let checksum = verifier
                .calculate_checksum(&mut reader, ChecksumAlgorithm::Crc32, None)
                .unwrap();

            assert_eq!(checksum.to_hex(), "3610a686");
        }

        #[test]
        fn test_verify_checksum_match() {
            let mut reader = Cursor::new(b"hello".to_vec());
            let mut verifier = Verifier::new();

            let result = verifier
                .verify_checksum(
                    &mut reader,
                    ChecksumAlgorithm::Md5,
                    "5d41402abc4b2a76b9719d911017c592",
                    Some(5),
                )
                .unwrap();

            assert!(result.success);
        }

        #[test]
        fn test_verify_checksum_mismatch() {
            let mut reader = Cursor::new(b"hello".to_vec());
            let mut verifier = Verifier::new();

            let result = verifier.verify_checksum(
                &mut reader,
                ChecksumAlgorithm::Md5,
                "00000000000000000000000000000000",
                Some(5),
            );

            assert!(matches!(result, Err(Error::ChecksumMismatch { .. })));
        }

        #[test]
        fn test_calculate_with_progress() {
            let data = vec![0u8; MIN_VERIFY_BLOCK_SIZE * 4];
            let mut reader = Cursor::new(data.clone());

            let progress_count = Arc::new(std::sync::atomic::AtomicU64::new(0));
            let progress_count_clone = Arc::clone(&progress_count);

            let config = VerifyConfig::new().block_size(MIN_VERIFY_BLOCK_SIZE);
            let mut verifier = Verifier::with_config(config).on_progress(move |_| {
                progress_count_clone.fetch_add(1, Ordering::SeqCst);
            });

            let _checksum = verifier
                .calculate_checksum(
                    &mut reader,
                    ChecksumAlgorithm::Sha256,
                    Some(data.len() as u64),
                )
                .unwrap();

            assert!(progress_count.load(Ordering::SeqCst) >= 4);
        }
    }

    // -------------------------------------------------------------------------
    // Checksum file parsing tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_parse_gnu_format() {
        let content =
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  file.iso\n\
                       d41d8cd98f00b204e9800998ecf8427e  other.img\n";

        let entries = parse_checksum_file(content);
        assert_eq!(entries.len(), 2);

        assert_eq!(entries[0].filename, "file.iso");
        assert_eq!(entries[0].algorithm, Some(ChecksumAlgorithm::Sha256));

        assert_eq!(entries[1].filename, "other.img");
        assert_eq!(entries[1].algorithm, Some(ChecksumAlgorithm::Md5));
    }

    #[test]
    fn test_parse_gnu_format_binary() {
        let content =
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855 *file.iso\n";

        let entries = parse_checksum_file(content);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].filename, "file.iso");
    }

    #[test]
    fn test_parse_bsd_format() {
        let content = "SHA256 (file.iso) = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855\n\
                       MD5 (other.img) = d41d8cd98f00b204e9800998ecf8427e\n";

        let entries = parse_checksum_file(content);
        assert_eq!(entries.len(), 2);

        assert_eq!(entries[0].filename, "file.iso");
        assert_eq!(entries[0].algorithm, Some(ChecksumAlgorithm::Sha256));

        assert_eq!(entries[1].filename, "other.img");
        assert_eq!(entries[1].algorithm, Some(ChecksumAlgorithm::Md5));
    }

    #[test]
    fn test_parse_with_comments() {
        let content = "# This is a comment\n\
                       e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  file.iso\n\
                       # Another comment\n";

        let entries = parse_checksum_file(content);
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn test_find_checksum_for_file() {
        let entries = vec![
            ChecksumEntry {
                checksum: "abc123".to_string(),
                filename: "file1.iso".to_string(),
                algorithm: Some(ChecksumAlgorithm::Sha256),
            },
            ChecksumEntry {
                checksum: "def456".to_string(),
                filename: "/path/to/file2.iso".to_string(),
                algorithm: Some(ChecksumAlgorithm::Sha256),
            },
        ];

        // Exact match
        let found = find_checksum_for_file(&entries, "file1.iso");
        assert!(found.is_some());
        assert_eq!(found.unwrap().checksum, "abc123");

        // Match by basename
        let found = find_checksum_for_file(&entries, "file2.iso");
        assert!(found.is_some());
        assert_eq!(found.unwrap().checksum, "def456");

        // No match
        let found = find_checksum_for_file(&entries, "nonexistent.iso");
        assert!(found.is_none());
    }

    // -------------------------------------------------------------------------
    // Helper function tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_bytes_to_hex() {
        assert_eq!(bytes_to_hex(&[]), "");
        assert_eq!(bytes_to_hex(&[0x00]), "00");
        assert_eq!(bytes_to_hex(&[0xff]), "ff");
        assert_eq!(bytes_to_hex(&[0x01, 0x23, 0x45]), "012345");
    }

    #[test]
    fn test_hex_to_bytes() {
        assert_eq!(hex_to_bytes("").unwrap(), Vec::<u8>::new());
        assert_eq!(hex_to_bytes("00").unwrap(), vec![0x00u8]);
        assert_eq!(hex_to_bytes("ff").unwrap(), vec![0xffu8]);
        assert_eq!(hex_to_bytes("FF").unwrap(), vec![0xffu8]);
        assert_eq!(hex_to_bytes("012345").unwrap(), vec![0x01u8, 0x23, 0x45]);
    }

    #[test]
    fn test_hex_to_bytes_invalid() {
        assert!(hex_to_bytes("0").is_err()); // Odd length
        assert!(hex_to_bytes("gg").is_err()); // Invalid chars
    }

    // -------------------------------------------------------------------------
    // Auto-detect checksum tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_auto_detect_checksum_direct_extension() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let iso_path = temp_dir.path().join("test.iso");
        let checksum_path = temp_dir.path().join("test.iso.sha256");

        // Create dummy files
        std::fs::write(&iso_path, b"test content").unwrap();
        std::fs::write(
            &checksum_path,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  test.iso\n",
        )
        .unwrap();

        let detected = auto_detect_checksum(iso_path.to_str().unwrap());
        assert!(detected.is_some());

        let detected = detected.unwrap();
        assert_eq!(detected.algorithm, ChecksumAlgorithm::Sha256);
        assert_eq!(
            detected.checksum,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(detected.source_file, checksum_path);
    }

    #[test]
    fn test_auto_detect_checksum_sums_file() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let iso_path = temp_dir.path().join("ubuntu.iso");
        let sums_path = temp_dir.path().join("SHA256SUMS");

        // Create dummy files
        // Note: SHA-256 checksums are exactly 64 hex characters
        std::fs::write(&iso_path, b"ubuntu content").unwrap();
        std::fs::write(
            &sums_path,
            "abc123def456abc123def456abc123def456abc123def456abc123def456abc12345  ubuntu.iso\n\
             def456abc123def456abc123def456abc123def456abc123def456abc123def45678  other.iso\n",
        )
        .unwrap();

        let detected = auto_detect_checksum(iso_path.to_str().unwrap());
        // This should fail because the checksums are 65 chars, not 64
        // The validation rejects invalid length checksums
        assert!(detected.is_none());
    }

    #[test]
    fn test_auto_detect_checksum_sums_file_valid() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let iso_path = temp_dir.path().join("ubuntu.iso");
        let sums_path = temp_dir.path().join("SHA256SUMS");

        // Create dummy files with valid 64-char SHA-256 checksums
        std::fs::write(&iso_path, b"ubuntu content").unwrap();
        std::fs::write(
            &sums_path,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  ubuntu.iso\n\
             d41d8cd98f00b204e9800998ecf8427e0000000000000000000000000000000a  other.iso\n",
        )
        .unwrap();

        let detected = auto_detect_checksum(iso_path.to_str().unwrap());
        assert!(detected.is_some());

        let detected = detected.unwrap();
        assert_eq!(detected.algorithm, ChecksumAlgorithm::Sha256);
        assert_eq!(
            detected.checksum,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_auto_detect_checksum_md5() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let iso_path = temp_dir.path().join("test.iso");
        let checksum_path = temp_dir.path().join("test.iso.md5");

        std::fs::write(&iso_path, b"test content").unwrap();
        std::fs::write(
            &checksum_path,
            "d41d8cd98f00b204e9800998ecf8427e  test.iso\n",
        )
        .unwrap();

        let detected = auto_detect_checksum(iso_path.to_str().unwrap());
        assert!(detected.is_some());

        let detected = detected.unwrap();
        assert_eq!(detected.algorithm, ChecksumAlgorithm::Md5);
        assert_eq!(detected.checksum, "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn test_auto_detect_checksum_not_found() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let iso_path = temp_dir.path().join("test.iso");
        std::fs::write(&iso_path, b"test content").unwrap();

        let detected = auto_detect_checksum(iso_path.to_str().unwrap());
        assert!(detected.is_none());
    }

    #[test]
    fn test_auto_detect_checksum_url_skipped() {
        // URLs should be skipped
        let detected = auto_detect_checksum("https://example.com/ubuntu.iso");
        assert!(detected.is_none());

        let detected = auto_detect_checksum("http://example.com/ubuntu.iso");
        assert!(detected.is_none());
    }

    #[test]
    fn test_auto_detect_checksum_sha256sum_extension() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let iso_path = temp_dir.path().join("test.iso");
        let checksum_path = temp_dir.path().join("test.iso.sha256sum");

        std::fs::write(&iso_path, b"test content").unwrap();
        std::fs::write(
            &checksum_path,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  test.iso\n",
        )
        .unwrap();

        let detected = auto_detect_checksum(iso_path.to_str().unwrap());
        assert!(detected.is_some());
        assert_eq!(detected.unwrap().algorithm, ChecksumAlgorithm::Sha256);
    }

    #[test]
    fn test_auto_detect_checksum_prefers_direct_extension() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let iso_path = temp_dir.path().join("test.iso");
        let direct_path = temp_dir.path().join("test.iso.sha256");
        let sums_path = temp_dir.path().join("SHA256SUMS");

        std::fs::write(&iso_path, b"test content").unwrap();
        // Direct extension has one checksum
        std::fs::write(
            &direct_path,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  test.iso\n",
        )
        .unwrap();
        // SUMS file has different checksum
        std::fs::write(
            &sums_path,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb  test.iso\n",
        )
        .unwrap();

        let detected = auto_detect_checksum(iso_path.to_str().unwrap());
        assert!(detected.is_some());
        // Should find the direct extension first
        assert_eq!(
            detected.unwrap().checksum,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
        );
    }

    // -------------------------------------------------------------------------
    // Legacy API tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_legacy_verify_write() {
        let data = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let mut source = Cursor::new(data.clone());
        let mut target = Cursor::new(data);

        let result = verify_write(&mut source, &mut target, 8, 4).unwrap();

        assert!(result.success);
        assert_eq!(result.bytes_verified, 8);
    }
}
