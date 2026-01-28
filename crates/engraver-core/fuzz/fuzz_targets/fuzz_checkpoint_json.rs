//! Fuzz test for checkpoint JSON parsing
//!
//! Tests that checkpoint deserialization handles arbitrary JSON safely.

#![no_main]

use libfuzzer_sys::fuzz_target;
use serde::{Deserialize, Serialize};

fuzz_target!(|data: &str| {
    // Test parsing the full checkpoint structure
    let result: Result<WriteCheckpoint, _> = serde_json::from_str(data);

    // If parsing succeeded, verify the result can be serialized back
    if let Ok(checkpoint) = result {
        // Should be able to serialize without panicking
        let _ = serde_json::to_string(&checkpoint);
        let _ = serde_json::to_string_pretty(&checkpoint);

        // Verify version field
        let _ = checkpoint.version;

        // Access fields that might have edge cases
        let _ = checkpoint.session_id.len();
        let _ = checkpoint.source_path.len();
        let _ = checkpoint.target_path.len();

        // Numeric fields should be accessible
        let _ = checkpoint.bytes_written;
        let _ = checkpoint.blocks_written;
        let _ = checkpoint.block_size;
        let _ = checkpoint.elapsed_seconds.is_finite();
    }

    // Test parsing just the config subset
    let _: Result<WriteConfigCheckpoint, _> = serde_json::from_str(data);

    // Test parsing source type
    let _: Result<SourceType, _> = serde_json::from_str(data);

    // Test with byte input (for binary JSON edge cases)
    let _: Result<WriteCheckpoint, _> = serde_json::from_slice(data.as_bytes());
});

/// Source type enumeration (mirrors engraver_core::SourceType)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceType {
    LocalFile,
    Remote,
    Gzip,
    Xz,
    Zstd,
    Bzip2,
}

/// Checkpoint structure (mirrors engraver_core::WriteCheckpoint)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteCheckpoint {
    pub version: u32,
    pub session_id: String,

    // Source Information
    pub source_path: String,
    pub source_type: SourceType,
    pub source_size: Option<u64>,
    pub source_header_hash: Option<String>,
    pub source_seekable: bool,
    pub source_resumable: bool,

    // Target Information
    pub target_path: String,
    pub target_size: u64,

    // Write Configuration
    pub block_size: usize,
    pub config: WriteConfigCheckpoint,

    // Progress State
    pub bytes_written: u64,
    pub blocks_written: u64,
    pub total_blocks: Option<u64>,

    // Timing Information
    pub start_time: u64,
    pub last_update: u64,
    pub elapsed_seconds: f64,

    // Retry Information
    pub resume_count: u32,
    pub total_retries: u32,
}

/// Serializable subset of WriteConfig (mirrors engraver_core::WriteConfigCheckpoint)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteConfigCheckpoint {
    pub block_size: usize,
    pub sync_each_block: bool,
    pub sync_on_complete: bool,
    pub retry_attempts: u32,
    pub verify: bool,
}
