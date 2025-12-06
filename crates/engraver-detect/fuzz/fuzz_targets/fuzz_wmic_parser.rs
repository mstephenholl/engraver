//! Fuzz test for Windows WMIC CSV parsing
//!
//! Tests that the CSV parsers handle arbitrary input without panicking.

#![no_main]

use libfuzzer_sys::fuzz_target;
use std::collections::HashMap;

fuzz_target!(|data: &str| {
    // Fuzz the disk CSV parser
    let _ = fuzz_parse_wmic_disks(data);
    
    // Fuzz the volume CSV parser
    let _ = fuzz_parse_wmic_volumes(data);
});

/// Physical disk info structure
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

/// Parse wmic diskdrive CSV output
fn fuzz_parse_wmic_disks(csv: &str) -> Vec<PhysicalDisk> {
    let mut disks = Vec::new();
    let mut headers: Vec<String> = Vec::new();

    for line in csv.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let fields: Vec<&str> = line.split(',').collect();

        if headers.is_empty() {
            headers = fields.iter().map(|s| s.to_string()).collect();
            continue;
        }

        if fields.len() != headers.len() {
            continue;
        }

        let mut row: HashMap<&str, &str> = HashMap::new();
        for (i, header) in headers.iter().enumerate() {
            if let Some(value) = fields.get(i) {
                row.insert(header.as_str(), *value);
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

        // Include all disks in fuzz testing (don't skip zero size)
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

/// Volume info structure
#[derive(Debug, Clone)]
struct VolumeInfo {
    drive_letter: String,
    label: Option<String>,
    filesystem: Option<String>,
    size: u64,
}

/// Parse volume CSV output
fn fuzz_parse_wmic_volumes(csv: &str) -> Vec<VolumeInfo> {
    let mut volumes = Vec::new();
    let mut headers: Vec<String> = Vec::new();

    for line in csv.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let fields: Vec<&str> = line.split(',').collect();

        if headers.is_empty() {
            headers = fields.iter().map(|s| s.to_string()).collect();
            continue;
        }

        if fields.len() != headers.len() {
            continue;
        }

        let mut row: HashMap<&str, &str> = HashMap::new();
        for (i, header) in headers.iter().enumerate() {
            if let Some(value) = fields.get(i) {
                row.insert(header.as_str(), *value);
            }
        }

        let drive_letter = row.get("DriveLetter").unwrap_or(&"").to_string();
        // Don't skip empty drive letters in fuzz testing

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
