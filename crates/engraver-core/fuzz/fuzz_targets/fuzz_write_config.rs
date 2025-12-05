//! Fuzz test for WriteConfig
//!
//! Tests that WriteConfig handles arbitrary inputs safely.

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use std::time::Duration;

/// Minimum block size (4 KB)
const MIN_BLOCK_SIZE: usize = 4 * 1024;

/// Maximum block size (64 MB)
const MAX_BLOCK_SIZE: usize = 64 * 1024 * 1024;

#[derive(Arbitrary, Debug)]
struct ConfigInput {
    block_size: usize,
    sync_each_block: bool,
    sync_on_complete: bool,
    retry_attempts: u32,
    retry_delay_ms: u64,
    verify: bool,
}

fuzz_target!(|input: ConfigInput| {
    // Build config with arbitrary values
    let config = WriteConfig::new()
        .block_size(input.block_size)
        .sync_each_block(input.sync_each_block)
        .sync_on_complete(input.sync_on_complete)
        .retry_attempts(input.retry_attempts)
        .retry_delay(Duration::from_millis(input.retry_delay_ms))
        .verify(input.verify);

    // Block size should be clamped to valid range
    assert!(
        config.block_size >= MIN_BLOCK_SIZE,
        "Block size {} < MIN {}",
        config.block_size,
        MIN_BLOCK_SIZE
    );
    assert!(
        config.block_size <= MAX_BLOCK_SIZE,
        "Block size {} > MAX {}",
        config.block_size,
        MAX_BLOCK_SIZE
    );

    // Boolean fields should match input
    assert_eq!(config.sync_each_block, input.sync_each_block);
    assert_eq!(config.sync_on_complete, input.sync_on_complete);
    assert_eq!(config.verify, input.verify);

    // Retry attempts should match (no clamping)
    assert_eq!(config.retry_attempts, input.retry_attempts);
});

/// WriteConfig replica for fuzzing
#[derive(Debug, Clone)]
struct WriteConfig {
    block_size: usize,
    sync_each_block: bool,
    sync_on_complete: bool,
    retry_attempts: u32,
    retry_delay: Duration,
    verify: bool,
}

impl Default for WriteConfig {
    fn default() -> Self {
        Self {
            block_size: 4 * 1024 * 1024,
            sync_each_block: false,
            sync_on_complete: true,
            retry_attempts: 3,
            retry_delay: Duration::from_millis(100),
            verify: false,
        }
    }
}

impl WriteConfig {
    fn new() -> Self {
        Self::default()
    }

    fn block_size(mut self, size: usize) -> Self {
        self.block_size = size.clamp(MIN_BLOCK_SIZE, MAX_BLOCK_SIZE);
        self
    }

    fn sync_each_block(mut self, sync: bool) -> Self {
        self.sync_each_block = sync;
        self
    }

    fn sync_on_complete(mut self, sync: bool) -> Self {
        self.sync_on_complete = sync;
        self
    }

    fn retry_attempts(mut self, attempts: u32) -> Self {
        self.retry_attempts = attempts;
        self
    }

    fn retry_delay(mut self, delay: Duration) -> Self {
        self.retry_delay = delay;
        self
    }

    fn verify(mut self, verify: bool) -> Self {
        self.verify = verify;
        self
    }
}
