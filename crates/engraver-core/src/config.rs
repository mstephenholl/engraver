//! Configuration types for Engraver

use serde::{Deserialize, Serialize};

/// Main configuration for Engraver operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Block size for read/write operations (default: 4MB)
    pub block_size: usize,

    /// Whether to verify after writing (default: true)
    pub verify: bool,

    /// Whether to sync after each block (default: false)
    pub sync_each_block: bool,

    /// Number of retry attempts on error (default: 3)
    pub retry_attempts: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            block_size: 4 * 1024 * 1024, // 4MB
            verify: true,
            sync_each_block: false,
            retry_attempts: 3,
        }
    }
}
