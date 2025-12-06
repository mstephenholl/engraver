//! Configuration for Engraver operations

use crate::writer::DEFAULT_BLOCK_SIZE;

/// Main configuration struct
#[derive(Debug, Clone)]
pub struct Config {
    /// Block size for read/write operations
    pub block_size: usize,

    /// Whether to verify writes after completion
    pub verify: bool,

    /// Whether to sync after each block
    pub sync_each_block: bool,

    /// Number of retry attempts on error
    pub retry_attempts: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            block_size: DEFAULT_BLOCK_SIZE,
            verify: true,
            sync_each_block: false,
            retry_attempts: 3,
        }
    }
}

impl Config {
    /// Create a new config with defaults
    pub fn new() -> Self {
        Self::default()
    }

    /// Set block size
    pub fn block_size(mut self, size: usize) -> Self {
        self.block_size = size;
        self
    }

    /// Set verify mode
    pub fn verify(mut self, verify: bool) -> Self {
        self.verify = verify;
        self
    }

    /// Set sync_each_block
    pub fn sync_each_block(mut self, sync: bool) -> Self {
        self.sync_each_block = sync;
        self
    }

    /// Set retry attempts
    pub fn retry_attempts(mut self, attempts: u32) -> Self {
        self.retry_attempts = attempts;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = Config::default();
        assert_eq!(config.block_size, DEFAULT_BLOCK_SIZE);
        assert!(config.verify);
        assert!(!config.sync_each_block);
        assert_eq!(config.retry_attempts, 3);
    }

    #[test]
    fn test_config_builder() {
        let config = Config::new()
            .block_size(1024 * 1024)
            .verify(false)
            .sync_each_block(true)
            .retry_attempts(5);

        assert_eq!(config.block_size, 1024 * 1024);
        assert!(!config.verify);
        assert!(config.sync_each_block);
        assert_eq!(config.retry_attempts, 5);
    }
}
