//! Fuzz test for path normalization functions
//!
//! Tests that path normalization handles arbitrary strings safely.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &str| {
    // Test Windows path normalization
    let normalized = normalize_windows_path(data);

    // Should never panic
    // Should always return a non-empty string (at minimum, the input)
    assert!(!normalized.is_empty() || data.is_empty());

    // If input was already a Windows device path, output should match
    if data.starts_with("\\\\.\\") {
        assert_eq!(normalized, data);
    }

    // Test macOS path conversion
    let raw_path = to_raw_device_path(data);

    // Should never panic
    assert!(!raw_path.is_empty() || data.is_empty());

    // Property: /dev/diskN becomes /dev/rdiskN
    if data.starts_with("/dev/disk") && !data.starts_with("/dev/rdisk") {
        assert!(raw_path.starts_with("/dev/rdisk"));
    }

    // Property: already raw paths are unchanged
    if data.starts_with("/dev/rdisk") {
        assert_eq!(raw_path, data);
    }
});

/// Windows path normalization (copy for fuzzing)
fn normalize_windows_path(path: &str) -> String {
    if path.starts_with("\\\\.\\") {
        path.to_string()
    } else if path.starts_with("PhysicalDrive") {
        format!("\\\\.\\{}", path)
    } else if let Ok(n) = path.parse::<u32>() {
        format!("\\\\.\\PhysicalDrive{}", n)
    } else {
        path.to_string()
    }
}

/// macOS raw device path conversion (copy for fuzzing)
fn to_raw_device_path(path: &str) -> String {
    if path.starts_with("/dev/disk") && !path.starts_with("/dev/rdisk") {
        path.replacen("/dev/disk", "/dev/rdisk", 1)
    } else {
        path.to_string()
    }
}
