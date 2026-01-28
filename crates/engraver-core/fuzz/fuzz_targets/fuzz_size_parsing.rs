//! Fuzz test for size string parsing
//!
//! Tests that size parsing functions handle arbitrary inputs safely.

#![no_main]

use libfuzzer_sys::fuzz_target;

use engraver_core::{parse_block_sizes, parse_size};

fuzz_target!(|data: &str| {
    // Test parse_size - should never panic, only return Ok/Err
    let result = parse_size(data);

    // If parsing succeeded, verify the result is reasonable
    if let Ok(size) = result {
        // Result should be a power of 2
        assert!(size.is_power_of_two(), "Size should be power of 2");
        // Result should be non-zero
        assert!(size > 0, "Size should be positive");
    }

    // Test parse_block_sizes with single value
    let _ = parse_block_sizes(data);

    // Test parse_block_sizes with multiple comma-separated values
    let multi_input = format!("{},{}", data, data);
    let _ = parse_block_sizes(&multi_input);

    // Test with common edge case patterns appended
    for suffix in ["", "B", "K", "KB", "M", "MB", "G", "GB", "k", "m", "g", "b"] {
        let test_input = format!("{}{}", data.trim(), suffix);
        let _ = parse_size(&test_input);
    }

    // Test with leading/trailing whitespace
    let whitespace_input = format!("  {}  ", data);
    let _ = parse_size(&whitespace_input);
});
