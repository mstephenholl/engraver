//! Post-write verification and checksum validation

use crate::Result;
use std::path::Path;

/// Checksum algorithm for verification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChecksumAlgorithm {
    /// SHA-256 (default, recommended)
    #[default]
    Sha256,
    /// SHA-512
    Sha512,
    /// MD5 (legacy, not recommended)
    Md5,
    /// CRC32 (fast but weak)
    Crc32,
}

/// Verification result
#[derive(Debug, Clone)]
pub struct VerificationResult {
    /// Whether verification passed
    pub success: bool,
    /// Source checksum
    pub source_checksum: String,
    /// Target checksum
    pub target_checksum: String,
    /// Algorithm used
    pub algorithm: ChecksumAlgorithm,
    /// Bytes verified
    pub bytes_verified: u64,
}

/// Verifier for post-write validation
pub struct Verifier {
    algorithm: ChecksumAlgorithm,
    block_size: usize,
}

impl Verifier {
    /// Create a new verifier with the specified algorithm
    pub fn new(algorithm: ChecksumAlgorithm) -> Self {
        Self {
            algorithm,
            block_size: 4 * 1024 * 1024,
        }
    }

    /// Verify that source and target match
    pub fn verify(&self, _source: &Path, _target: &Path, _size: u64) -> Result<VerificationResult> {
        todo!("Implement verification")
    }

    /// Calculate checksum of a file or device
    pub fn checksum(&self, _path: &Path, _size: Option<u64>) -> Result<String> {
        todo!("Implement checksum calculation")
    }
}

impl Default for Verifier {
    fn default() -> Self {
        Self::new(ChecksumAlgorithm::default())
    }
}
