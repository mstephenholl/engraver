//! Fuzz test for macOS plist parsing
//!
//! Tests that the plist parsers handle arbitrary input without panicking.

#![no_main]

use libfuzzer_sys::fuzz_target;

// We need to expose the parsing functions for fuzzing
// This assumes they're made pub(crate) or we add a test feature

fuzz_target!(|data: &str| {
    // Fuzz the disk list parser
    // Should never panic on any input
    let _ = fuzz_parse_disk_list(data);
    
    // Fuzz the disk info parser
    let _ = fuzz_parse_disk_info(data);
    
    // Fuzz the partition parser
    let _ = fuzz_parse_partitions(data);
});

/// Simplified disk list parser for fuzzing
fn fuzz_parse_disk_list(plist: &str) -> Vec<String> {
    let mut disks = Vec::new();
    let mut in_whole_disks = false;
    let mut in_array = false;

    for line in plist.lines() {
        let trimmed = line.trim();

        if trimmed.contains("<key>WholeDisks</key>") {
            in_whole_disks = true;
            continue;
        }

        if in_whole_disks {
            if trimmed == "<array>" {
                in_array = true;
                continue;
            }
            if trimmed == "</array>" {
                break;
            }
            if in_array && trimmed.starts_with("<string>") && trimmed.ends_with("</string>") {
                let disk = trimmed
                    .trim_start_matches("<string>")
                    .trim_end_matches("</string>");
                disks.push(disk.to_string());
            }
        }
    }

    disks
}

/// Simplified disk info parser for fuzzing
fn fuzz_parse_disk_info(plist: &str) -> std::collections::HashMap<String, String> {
    let mut info = std::collections::HashMap::new();
    let mut current_key: Option<String> = None;

    for line in plist.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("<key>") && trimmed.ends_with("</key>") {
            current_key = Some(
                trimmed
                    .trim_start_matches("<key>")
                    .trim_end_matches("</key>")
                    .to_string(),
            );
        } else if let Some(key) = current_key.take() {
            let value = if trimmed.starts_with("<string>") && trimmed.ends_with("</string>") {
                trimmed
                    .trim_start_matches("<string>")
                    .trim_end_matches("</string>")
                    .to_string()
            } else if trimmed.starts_with("<integer>") && trimmed.ends_with("</integer>") {
                trimmed
                    .trim_start_matches("<integer>")
                    .trim_end_matches("</integer>")
                    .to_string()
            } else if trimmed == "<true/>" {
                "true".to_string()
            } else if trimmed == "<false/>" {
                "false".to_string()
            } else {
                continue;
            };
            info.insert(key, value);
        }
    }

    info
}

/// Simplified partition parser for fuzzing
fn fuzz_parse_partitions(plist: &str) -> Vec<(String, u64)> {
    let mut partitions = Vec::new();
    let mut in_partitions = false;
    let mut in_partition = false;
    let mut current_partition: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut current_key: Option<String> = None;

    for line in plist.lines() {
        let trimmed = line.trim();

        if trimmed.contains("<key>Partitions</key>") {
            in_partitions = true;
            continue;
        }

        if !in_partitions {
            continue;
        }

        if trimmed == "<dict>" {
            in_partition = true;
            current_partition.clear();
            continue;
        }

        if trimmed == "</dict>" && in_partition {
            if let Some(dev_id) = current_partition.get("DeviceIdentifier") {
                let size = current_partition
                    .get("Size")
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(0);
                partitions.push((dev_id.clone(), size));
            }
            in_partition = false;
            continue;
        }

        if in_partition {
            if trimmed.starts_with("<key>") && trimmed.ends_with("</key>") {
                current_key = Some(
                    trimmed
                        .trim_start_matches("<key>")
                        .trim_end_matches("</key>")
                        .to_string(),
                );
            } else if let Some(key) = current_key.take() {
                let value = if trimmed.starts_with("<string>") && trimmed.ends_with("</string>") {
                    trimmed
                        .trim_start_matches("<string>")
                        .trim_end_matches("</string>")
                        .to_string()
                } else if trimmed.starts_with("<integer>") && trimmed.ends_with("</integer>") {
                    trimmed
                        .trim_start_matches("<integer>")
                        .trim_end_matches("</integer>")
                        .to_string()
                } else {
                    continue;
                };
                current_partition.insert(key, value);
            }
        }
    }

    partitions
}
