//! Verification and checksum module for Engraver
//!
//! This module handles:
//! - Post-write verification (read-back and compare)
//! - Checksum calculation (SHA256, SHA512, MD5)
//! - Progress tracking during verification
//!
//! TODO: Full implementation in next phase

use crate::error::{Error, Result};
use std::io::{Read, Seek, SeekFrom};

/// Supported checksum algorithms
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ChecksumAlgorithm {
    /// SHA-256
    Sha256,
    /// SHA-512
    Sha512,
    /// MD5 (legacy, not recommended)
    Md5,
    /// CRC32 (fast, not cryptographic)
    Crc32,
}

impl ChecksumAlgorithm {
    /// Get the expected output length in hex characters
    pub fn hex_length(&self) -> usize {
        match self {
            ChecksumAlgorithm::Sha256 => 64,
            ChecksumAlgorithm::Sha512 => 128,
            ChecksumAlgorithm::Md5 => 32,
            ChecksumAlgorithm::Crc32 => 8,
        }
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
}

impl std::fmt::Display for ChecksumAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Verification result
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// Whether verification passed
    pub success: bool,

    /// Bytes verified
    pub bytes_verified: u64,

    /// Number of mismatches found
    pub mismatches: u64,

    /// First mismatch offset (if any)
    pub first_mismatch_offset: Option<u64>,
}

/// Verification progress
#[derive(Debug, Clone)]
pub struct VerificationProgress {
    /// Bytes verified so far
    pub bytes_verified: u64,

    /// Total bytes to verify
    pub total_bytes: u64,

    /// Current speed in bytes per second
    pub speed_bps: u64,
}

impl VerificationProgress {
    /// Calculate completion percentage
    pub fn percentage(&self) -> f64 {
        if self.total_bytes == 0 {
            100.0
        } else {
            (self.bytes_verified as f64 / self.total_bytes as f64) * 100.0
        }
    }
}

/// Verify that target matches source by reading back
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
    // Seek both to start
    source.seek(SeekFrom::Start(0))?;
    target.seek(SeekFrom::Start(0))?;

    let mut source_buf = vec![0u8; block_size];
    let mut target_buf = vec![0u8; block_size];
    let mut bytes_verified = 0u64;
    let mut mismatches = 0u64;
    let mut first_mismatch: Option<u64> = None;

    while bytes_verified < size {
        let to_read = block_size.min((size - bytes_verified) as usize);

        let source_read = read_full(source, &mut source_buf[..to_read])?;
        let target_read = read_full(target, &mut target_buf[..to_read])?;

        if source_read != target_read {
            mismatches += 1;
            if first_mismatch.is_none() {
                first_mismatch = Some(bytes_verified);
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
        }

        bytes_verified += source_read as u64;

        if source_read < to_read {
            break; // EOF
        }
    }

    Ok(VerificationResult {
        success: mismatches == 0,
        bytes_verified,
        mismatches,
        first_mismatch_offset: first_mismatch,
    })
}

/// Read full buffer or until EOF
fn read_full<R: Read>(reader: &mut R, buf: &mut [u8]) -> Result<usize> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_checksum_algorithm_hex_length() {
        assert_eq!(ChecksumAlgorithm::Sha256.hex_length(), 64);
        assert_eq!(ChecksumAlgorithm::Sha512.hex_length(), 128);
        assert_eq!(ChecksumAlgorithm::Md5.hex_length(), 32);
        assert_eq!(ChecksumAlgorithm::Crc32.hex_length(), 8);
    }

    #[test]
    fn test_checksum_algorithm_name() {
        assert_eq!(ChecksumAlgorithm::Sha256.name(), "SHA-256");
        assert_eq!(ChecksumAlgorithm::Sha512.name(), "SHA-512");
        assert_eq!(ChecksumAlgorithm::Md5.name(), "MD5");
        assert_eq!(ChecksumAlgorithm::Crc32.name(), "CRC32");
    }

    #[test]
    fn test_checksum_algorithm_display() {
        assert_eq!(format!("{}", ChecksumAlgorithm::Sha256), "SHA-256");
    }

    #[test]
    fn test_verification_progress_percentage() {
        let progress = VerificationProgress {
            bytes_verified: 500,
            total_bytes: 1000,
            speed_bps: 0,
        };
        assert_eq!(progress.percentage(), 50.0);
    }

    #[test]
    fn test_verification_progress_percentage_zero() {
        let progress = VerificationProgress {
            bytes_verified: 0,
            total_bytes: 0,
            speed_bps: 0,
        };
        assert_eq!(progress.percentage(), 100.0);
    }

    #[test]
    fn test_verify_write_matching() {
        let data = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let mut source = Cursor::new(data.clone());
        let mut target = Cursor::new(data);

        let result = verify_write(&mut source, &mut target, 8, 4).unwrap();

        assert!(result.success);
        assert_eq!(result.bytes_verified, 8);
        assert_eq!(result.mismatches, 0);
        assert!(result.first_mismatch_offset.is_none());
    }

    #[test]
    fn test_verify_write_mismatch() {
        let source_data = vec![1u8, 2, 3, 4, 5, 6, 7, 8];
        let target_data = vec![1u8, 2, 3, 4, 5, 6, 7, 9]; // Last byte different

        let mut source = Cursor::new(source_data);
        let mut target = Cursor::new(target_data);

        let result = verify_write(&mut source, &mut target, 8, 4).unwrap();

        assert!(!result.success);
        assert_eq!(result.mismatches, 1);
        assert_eq!(result.first_mismatch_offset, Some(7));
    }

    #[test]
    fn test_verify_write_early_mismatch() {
        let source_data = vec![1u8, 2, 3, 4];
        let target_data = vec![0u8, 2, 3, 4]; // First byte different

        let mut source = Cursor::new(source_data);
        let mut target = Cursor::new(target_data);

        let result = verify_write(&mut source, &mut target, 4, 2).unwrap();

        assert!(!result.success);
        assert_eq!(result.first_mismatch_offset, Some(0));
    }

    #[test]
    fn test_verify_write_empty() {
        let mut source = Cursor::new(Vec::<u8>::new());
        let mut target = Cursor::new(Vec::<u8>::new());

        let result = verify_write(&mut source, &mut target, 0, 4).unwrap();

        assert!(result.success);
        assert_eq!(result.bytes_verified, 0);
    }
}
