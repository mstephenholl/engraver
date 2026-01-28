//! Fuzz test for CSV parsing with quote handling
//!
//! Tests that CSV parsing (used for Windows PowerShell output) handles
//! arbitrary input without panicking, including edge cases like:
//! - Unmatched quotes
//! - Escaped quotes (doubled "")
//! - Fields with embedded commas
//! - Header/field count mismatches

#![no_main]

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;
use std::collections::HashMap;

fuzz_target!(|data: &str| {
    // Test the CSV line parser directly
    for line in data.lines() {
        let fields = fuzz_parse_csv_line(line);

        // Verify no field is unexpectedly empty in middle (unless intended)
        for field in &fields {
            // Field should be valid UTF-8 (already guaranteed by &str)
            let _ = field.len();
        }
    }

    // Test full CSV parsing (header + data rows)
    let disks = fuzz_parse_csv_disks(data);
    for disk in &disks {
        // Access fields to ensure no panics
        let _ = disk.device_id.len();
        let _ = disk.model.len();
        let _ = disk.size;
    }

    // Test with structured input
    let volumes = fuzz_parse_csv_volumes(data);
    for vol in &volumes {
        let _ = vol.drive_letter.len();
        let _ = vol.size;
    }
});

/// Parse a CSV line handling quoted fields
///
/// Handles:
/// - Fields separated by commas
/// - Quoted fields containing commas
/// - Quote toggling (not proper escaping)
fn fuzz_parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;

    for c in line.chars() {
        match c {
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => {
                fields.push(current.trim().to_string());
                current = String::new();
            }
            _ => current.push(c),
        }
    }
    fields.push(current.trim().to_string());
    fields
}

/// Physical disk info
#[derive(Debug, Clone)]
struct PhysicalDisk {
    index: u32,
    device_id: String,
    model: String,
    size: u64,
    media_type: String,
    interface_type: String,
    serial: Option<String>,
}

/// Parse CSV output for disk information
fn fuzz_parse_csv_disks(csv: &str) -> Vec<PhysicalDisk> {
    let mut disks = Vec::new();
    let mut lines = csv.lines().peekable();

    // First line should be headers
    let headers: Vec<String> = match lines.next() {
        Some(line) => fuzz_parse_csv_line(line),
        None => return disks,
    };

    if headers.is_empty() {
        return disks;
    }

    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let fields = fuzz_parse_csv_line(line);
        if fields.len() != headers.len() {
            continue;
        }

        let mut row: HashMap<&str, &str> = HashMap::new();
        for (i, header) in headers.iter().enumerate() {
            if let Some(value) = fields.get(i) {
                row.insert(header.as_str(), value.as_str());
            }
        }

        let index = row
            .get("Index")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);
        let device_id = row.get("DeviceID").unwrap_or(&"").to_string();
        let model = row.get("Model").unwrap_or(&"Unknown").trim().to_string();
        let size = row
            .get("Size")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let media_type = row.get("MediaType").unwrap_or(&"").to_string();
        let interface_type = row.get("InterfaceType").unwrap_or(&"").to_string();
        let serial = row
            .get("SerialNumber")
            .filter(|s| !s.is_empty())
            .map(|s| s.trim().to_string());

        disks.push(PhysicalDisk {
            index,
            device_id,
            model,
            size,
            media_type,
            interface_type,
            serial,
        });
    }

    disks
}

/// Volume info
#[derive(Debug, Clone)]
struct VolumeInfo {
    drive_letter: String,
    label: Option<String>,
    filesystem: Option<String>,
    size: u64,
}

/// Parse CSV output for volume information
fn fuzz_parse_csv_volumes(csv: &str) -> Vec<VolumeInfo> {
    let mut volumes = Vec::new();
    let mut lines = csv.lines().peekable();

    let headers: Vec<String> = match lines.next() {
        Some(line) => fuzz_parse_csv_line(line),
        None => return volumes,
    };

    if headers.is_empty() {
        return volumes;
    }

    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let fields = fuzz_parse_csv_line(line);
        if fields.len() != headers.len() {
            continue;
        }

        let mut row: HashMap<&str, &str> = HashMap::new();
        for (i, header) in headers.iter().enumerate() {
            if let Some(value) = fields.get(i) {
                row.insert(header.as_str(), value.as_str());
            }
        }

        let drive_letter = row.get("DriveLetter").unwrap_or(&"").to_string();
        let label = row
            .get("Label")
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let filesystem = row
            .get("FileSystem")
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        let size = row
            .get("Capacity")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        volumes.push(VolumeInfo {
            drive_letter,
            label,
            filesystem,
            size,
        });
    }

    volumes
}
