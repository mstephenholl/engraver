//! Fuzz test for hex string parsing
//!
//! Tests that hex-to-bytes conversion handles arbitrary inputs safely.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    // Test the hex parsing function
    let result = fuzz_hex_to_bytes(data);

    // If parsing succeeded, verify invariants
    if let Ok(bytes) = &result {
        // Output length should be half the input length (for valid hex)
        assert_eq!(bytes.len(), data.len() / 2);

        // Verify round-trip: bytes back to hex should give lowercase original
        let roundtrip: String = bytes.iter().map(|b| format!("{:02x}", b)).collect();
        assert_eq!(roundtrip, data.to_lowercase());
    }

    // Test with various transformations
    let uppercase = data.to_uppercase();
    let _ = fuzz_hex_to_bytes(&uppercase);

    let lowercase = data.to_lowercase();
    let _ = fuzz_hex_to_bytes(&lowercase);

    // Test with whitespace (should fail)
    let with_spaces = format!(" {} ", data);
    let _ = fuzz_hex_to_bytes(&with_spaces);

    // Test with prefix (should fail for 0x prefix)
    let with_prefix = format!("0x{}", data);
    let _ = fuzz_hex_to_bytes(&with_prefix);
});

/// Convert hex string to bytes (mirrors internal implementation)
fn fuzz_hex_to_bytes(hex: &str) -> Result<Vec<u8>, HexError> {
    // Check for even length
    if hex.len() % 2 != 0 {
        return Err(HexError::OddLength);
    }

    // Check for empty string
    if hex.is_empty() {
        return Ok(Vec::new());
    }

    // Parse pairs of hex characters
    (0..hex.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&hex[i..i + 2], 16)
                .map_err(|_| HexError::InvalidCharacter(i))
        })
        .collect()
}

/// Errors from hex parsing
#[derive(Debug)]
enum HexError {
    /// Hex string has odd length
    OddLength,
    /// Invalid hex character at position
    InvalidCharacter(usize),
}
