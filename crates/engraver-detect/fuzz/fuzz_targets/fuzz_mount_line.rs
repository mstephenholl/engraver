//! Fuzz test for Linux mount line parsing
//!
//! Tests that mount line parsing handles arbitrary input without panicking,
//! including edge cases specific to /proc/mounts format.

#![no_main]

use libfuzzer_sys::fuzz_target;
use std::collections::HashMap;

fuzz_target!(|data: &str| {
    // Test parsing individual mount lines
    for line in data.lines() {
        let result = fuzz_parse_mount_line(line);

        // If parsing succeeded, verify the result is reasonable
        if let Some((device, info)) = result {
            // Device and mount point should be non-empty (by our logic)
            assert!(!device.is_empty());
            assert!(!info.mount_point.is_empty());
        }
    }

    // Test full mount file parsing
    let mounts = fuzz_parse_mounts(data);
    for (device, info) in &mounts {
        // Access fields to ensure no panics
        let _ = device.len();
        let _ = info.mount_point.len();
        if let Some(ref fs) = info.filesystem {
            let _ = fs.len();
        }
    }

    // Test device name filtering
    for line in data.lines() {
        let trimmed = line.trim();
        if !trimmed.is_empty() {
            let _ = fuzz_should_skip_device(trimmed);
        }
    }

    // Test label decoding (handles \xNN escape sequences)
    for line in data.lines() {
        let decoded = fuzz_decode_label(line);
        // Decoded label should be valid UTF-8
        let _ = decoded.len();
    }

    // Test system mount point detection
    for line in data.lines() {
        let _ = fuzz_is_system_mount_point(line.trim());
    }
});

/// Mount information for a device
#[derive(Debug, Clone)]
struct MountInfo {
    pub mount_point: String,
    pub filesystem: Option<String>,
}

/// Parse a single line from /proc/mounts
/// Format: device mount_point filesystem options dump pass
fn fuzz_parse_mount_line(line: &str) -> Option<(String, MountInfo)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 3 {
        let device = parts[0].to_string();
        let mount_point = parts[1].to_string();
        let filesystem = parts[2].to_string();

        // Filter out pseudo filesystems
        let fs = if filesystem == "devtmpfs"
            || filesystem == "sysfs"
            || filesystem == "proc"
            || filesystem == "tmpfs"
            || filesystem == "securityfs"
            || filesystem == "cgroup2"
        {
            None
        } else {
            Some(filesystem)
        };

        Some((
            device,
            MountInfo {
                mount_point,
                filesystem: fs,
            },
        ))
    } else {
        None
    }
}

/// Parse entire mount file content
fn fuzz_parse_mounts(content: &str) -> HashMap<String, MountInfo> {
    let mut mounts = HashMap::new();

    for line in content.lines() {
        if let Some((device, info)) = fuzz_parse_mount_line(line) {
            mounts.insert(device, info);
        }
    }

    mounts
}

/// Check if a device should be skipped
fn fuzz_should_skip_device(name: &str) -> bool {
    name.starts_with("loop")
        || name.starts_with("ram")
        || name.starts_with("dm-")
        || name.starts_with("zram")
        || name.starts_with("sr")  // CD/DVD drives
        || name.starts_with("fd") // Floppy drives
}

/// Decode URL-encoded label (handles \xNN style escapes)
fn fuzz_decode_label(label: &str) -> String {
    let mut result = String::new();
    let mut chars = label.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            // Check for \xNN escape sequence
            if chars.peek() == Some(&'x') {
                chars.next(); // consume 'x'

                // Try to read two hex digits
                let mut hex = String::new();
                for _ in 0..2 {
                    if let Some(&c) = chars.peek() {
                        if c.is_ascii_hexdigit() {
                            hex.push(chars.next().unwrap());
                        } else {
                            break;
                        }
                    }
                }

                if hex.len() == 2 {
                    if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                        if byte.is_ascii() {
                            result.push(byte as char);
                            continue;
                        }
                    }
                }

                // Invalid escape sequence, output as-is
                result.push('\\');
                result.push('x');
                result.push_str(&hex);
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }

    result
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

/// Check if a mount point indicates a system drive
fn fuzz_is_system_mount_point(mount_point: &str) -> bool {
    let normalized = mount_point.trim();

    SYSTEM_MOUNT_POINTS.iter().any(|&sys| {
        normalized == sys
            || normalized.eq_ignore_ascii_case(sys)
            || normalized.starts_with(&format!("{}\\", sys))
            || normalized.starts_with(&format!("{}/", sys))
    })
}
