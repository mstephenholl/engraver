//! Fuzz test for Linux /proc/mounts parsing
//!
//! Tests that the mount parser handles arbitrary input without panicking.

#![no_main]

use libfuzzer_sys::fuzz_target;
use std::collections::HashMap;

fuzz_target!(|data: &str| {
    // Fuzz the mount line parser
    for line in data.lines() {
        let _ = fuzz_parse_mount_line(line);
    }
    
    // Fuzz the full mount file parser
    let _ = fuzz_parse_mounts(data);
    
    // Fuzz the system mount point checker
    for line in data.lines() {
        let _ = fuzz_is_system_mount_point(line.trim());
    }
});

/// Parse a single line from /proc/mounts
fn fuzz_parse_mount_line(line: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 2 {
        Some((parts[0].to_string(), parts[1].to_string()))
    } else {
        None
    }
}

/// Parse entire mount file content
fn fuzz_parse_mounts(content: &str) -> HashMap<String, String> {
    let mut mounts = HashMap::new();

    for line in content.lines() {
        if let Some((device, mount_point)) = fuzz_parse_mount_line(line) {
            mounts.insert(device, mount_point);
        }
    }

    mounts
}

/// System mount points that indicate a system drive
const SYSTEM_MOUNT_POINTS: &[&str] = &[
    "/",
    "/boot",
    "/boot/efi",
    "/home",
    "/usr",
    "/var",
    "/etc",
    "/System",
    "/Applications",
    "/Library",
    "C:\\",
    "C:\\Windows",
];

/// Check if any mount point indicates a system drive
fn fuzz_is_system_mount_point(mount_point: &str) -> bool {
    let normalized = mount_point.trim();
    
    SYSTEM_MOUNT_POINTS
        .iter()
        .any(|&sys| {
            normalized == sys 
            || normalized.eq_ignore_ascii_case(sys)
            || normalized.starts_with(&format!("{}\\", sys))
            || normalized.starts_with(&format!("{}/", sys))
        })
}
