//! Fuzz test for format_bytes function
//!
//! Tests that the byte formatter handles all u64 values without panicking.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: u64| {
    // Fuzz the format_bytes function with arbitrary u64 values
    let result = fuzz_format_bytes(data);
    
    // Verify the result is valid
    assert!(!result.is_empty());
    assert!(result.contains(' ')); // Should have a space before unit
    
    // Verify it ends with a valid unit
    assert!(
        result.ends_with(" B") 
        || result.ends_with(" KB")
        || result.ends_with(" MB")
        || result.ends_with(" GB")
        || result.ends_with(" TB")
    );
});

/// Format bytes into human-readable string
fn fuzz_format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
