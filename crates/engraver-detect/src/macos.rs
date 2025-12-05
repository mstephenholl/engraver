//! macOS drive detection implementation
//!
//! Uses `diskutil` command for device enumeration and information.

use super::{is_system_mount_point, DetectError, Drive, DriveType, Partition, Result, UsbSpeed};
use std::collections::HashMap;
use std::process::Command;

/// List all drives on macOS
pub fn list_drives() -> Result<Vec<Drive>> {
    let output = Command::new("diskutil")
        .args(["list", "-plist"])
        .output()
        .map_err(|e| DetectError::CommandFailed(format!("diskutil list failed: {}", e)))?;

    if !output.status.success() {
        return Err(DetectError::CommandFailed(format!(
            "diskutil list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let plist_str = String::from_utf8_lossy(&output.stdout);
    let disk_names = parse_disk_list(&plist_str)?;

    let mut drives = Vec::new();

    for disk_name in disk_names {
        match get_disk_info(&disk_name) {
            Ok(Some(drive)) => drives.push(drive),
            Ok(None) => continue,
            Err(e) => {
                tracing::debug!("Failed to get info for {}: {}", disk_name, e);
                continue;
            }
        }
    }

    Ok(drives)
}

/// Parse disk list from diskutil plist output
pub(crate) fn parse_disk_list(plist: &str) -> Result<Vec<String>> {
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

    if disks.is_empty() {
        // Try AllDisks as fallback
        let mut in_all_disks = false;
        for line in plist.lines() {
            let trimmed = line.trim();

            if trimmed.contains("<key>AllDisks</key>") {
                in_all_disks = true;
                continue;
            }

            if in_all_disks {
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
                    // Only add whole disks (no 's' partition suffix)
                    if !disk.contains('s')
                        || disk.starts_with("disk")
                            && disk.chars().filter(|c| *c == 's').count() == 0
                    {
                        disks.push(disk.to_string());
                    }
                }
            }
        }
    }

    // Deduplicate
    disks.sort();
    disks.dedup();

    if disks.is_empty() {
        return Err(DetectError::ParseError(
            "No disks found in diskutil output".to_string(),
        ));
    }

    Ok(disks)
}

/// Get detailed info for a specific disk
fn get_disk_info(disk_name: &str) -> Result<Option<Drive>> {
    let output = Command::new("diskutil")
        .args(["info", "-plist", disk_name])
        .output()
        .map_err(|e| DetectError::CommandFailed(format!("diskutil info failed: {}", e)))?;

    if !output.status.success() {
        return Ok(None);
    }

    let plist_str = String::from_utf8_lossy(&output.stdout);
    let info = parse_disk_info(&plist_str)?;

    // Skip virtual/synthesized disks
    if info.get("VirtualOrPhysical").map(|s| s.as_str()) == Some("Virtual") {
        return Ok(None);
    }

    // Skip APFS containers
    if info.get("APFSContainerReference").is_some() && !info.contains_key("DeviceNode") {
        return Ok(None);
    }

    let device_node = match info.get("DeviceNode") {
        Some(node) => node.clone(),
        None => return Ok(None),
    };

    let size = info
        .get("TotalSize")
        .or_else(|| info.get("Size"))
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    if size == 0 {
        return Ok(None);
    }

    let removable = info
        .get("RemovableMedia")
        .map(|s| s == "true")
        .unwrap_or(false)
        || info.get("Ejectable").map(|s| s == "true").unwrap_or(false);

    let internal = info.get("Internal").map(|s| s == "true").unwrap_or(true);

    let vendor = info.get("MediaName").cloned();
    let model = info.get("IORegistryEntryName").cloned();

    let drive_type = detect_drive_type(&info);
    let partitions = get_disk_partitions(disk_name)?;

    let mount_points: Vec<String> = partitions
        .iter()
        .filter_map(|p| p.mount_point.clone())
        .collect();

    let (is_system, system_reason) = check_if_system_drive(&info, &mount_points, internal);

    // Get USB speed for USB devices
    let usb_speed = if drive_type == DriveType::Usb {
        get_usb_speed_for_disk(disk_name, &info)
    } else {
        None
    };

    let display_name = vendor
        .clone()
        .or_else(|| model.clone())
        .unwrap_or_else(|| disk_name.to_string());

    let raw_path = format!("/dev/r{}", disk_name);

    Ok(Some(Drive {
        path: device_node,
        raw_path,
        name: display_name.clone(),
        size,
        removable,
        is_system,
        drive_type,
        vendor,
        model,
        serial: None,
        mount_points,
        partitions,
        system_reason,
        usb_speed,
    }))
}

/// Parse disk info plist into a key-value map
pub(crate) fn parse_disk_info(plist: &str) -> Result<HashMap<String, String>> {
    let mut info = HashMap::new();
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

    Ok(info)
}

/// Get partitions for a disk
fn get_disk_partitions(disk_name: &str) -> Result<Vec<Partition>> {
    let output = Command::new("diskutil")
        .args(["list", "-plist", disk_name])
        .output()
        .map_err(|e| DetectError::CommandFailed(format!("diskutil list failed: {}", e)))?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let plist_str = String::from_utf8_lossy(&output.stdout);
    parse_partitions(&plist_str, disk_name)
}

/// Parse partitions from diskutil list output
pub(crate) fn parse_partitions(plist: &str, disk_name: &str) -> Result<Vec<Partition>> {
    let mut partitions = Vec::new();

    let mut in_partitions = false;
    let mut in_partition = false;
    let mut current_partition: HashMap<String, String> = HashMap::new();
    let mut current_key: Option<String> = None;

    for line in plist.lines() {
        let trimmed = line.trim();

        if trimmed.contains("<key>AllDisksAndPartitions</key>")
            || trimmed.contains("<key>Partitions</key>")
        {
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
                if dev_id != disk_name {
                    let size = current_partition
                        .get("Size")
                        .and_then(|s| s.parse::<u64>().ok())
                        .unwrap_or(0);

                    let mount_point = current_partition.get("MountPoint").cloned();

                    partitions.push(Partition {
                        path: format!("/dev/{}", dev_id),
                        label: current_partition.get("VolumeName").cloned(),
                        filesystem: current_partition.get("Content").cloned(),
                        size,
                        mount_point,
                    });
                }
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

    Ok(partitions)
}

/// Detect drive type from disk info
pub(crate) fn detect_drive_type(info: &HashMap<String, String>) -> DriveType {
    let protocol = info.get("DeviceProtocol").map(|s| s.as_str());
    let bus = info.get("BusProtocol").map(|s| s.as_str());
    let media_name = info.get("MediaName").map(|s| s.to_lowercase());

    match protocol.or(bus) {
        Some("USB") => return DriveType::Usb,
        Some("NVMe") | Some("PCI-Express") => return DriveType::Nvme,
        Some("SATA") | Some("SAS") => return DriveType::Sata,
        Some("Secure Digital") | Some("SD") => return DriveType::SdCard,
        _ => {}
    }

    if let Some(name) = media_name {
        if name.contains("sd") || name.contains("card") {
            return DriveType::SdCard;
        }
    }

    DriveType::Other
}

/// Get USB speed for a device by querying system_profiler
///
/// Parses `system_profiler SPUSBDataType` output to find the speed
/// of the USB device matching the given disk name.
fn get_usb_speed_for_disk(disk_name: &str, info: &HashMap<String, String>) -> Option<UsbSpeed> {
    // Only check USB devices
    let protocol = info
        .get("DeviceProtocol")
        .or_else(|| info.get("BusProtocol"));
    if protocol.map(|s| s.as_str()) != Some("USB") {
        return None;
    }

    // Get the media name to help match in USB tree
    let media_name = info.get("MediaName").cloned();

    // Run system_profiler to get USB device info
    let output = Command::new("system_profiler")
        .args(["SPUSBDataType", "-detailLevel", "mini"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    parse_usb_speed_from_profiler(&output_str, disk_name, media_name.as_deref())
}

/// Parse USB speed from system_profiler output
///
/// The output format is hierarchical text like:
/// ```text
/// USB 3.0 Bus:
///   Host Controller Driver: AppleUSBXHCI
///   USB3.1 Hub:
///     Product ID: 0x0612
///     Speed: Up to 10 Gb/s
///     SanDisk Ultra:
///       Product ID: 0x5591
///       Speed: Up to 5 Gb/s
/// ```
pub(crate) fn parse_usb_speed_from_profiler(
    output: &str,
    _disk_name: &str,
    media_name: Option<&str>,
) -> Option<UsbSpeed> {
    // Look for the device by media name in the USB tree
    let search_name = media_name?;

    let mut found_device = false;
    let mut in_device_section = false;

    for line in output.lines() {
        let trimmed = line.trim();

        // Check if this line contains our device name
        if !found_device && trimmed.contains(search_name) {
            found_device = true;
            in_device_section = true;
            continue;
        }

        // Once we found the device, look for Speed line
        if in_device_section {
            // Check if we've moved to a different device (less indentation or new device header)
            if !line.starts_with(' ') && !line.is_empty() {
                // Left the device section
                break;
            }

            if trimmed.starts_with("Speed:") {
                let speed_str = trimmed.trim_start_matches("Speed:").trim();
                return parse_speed_string(speed_str);
            }
        }
    }

    None
}

/// Parse a speed string like "Up to 5 Gb/s" or "Up to 480 Mb/s"
fn parse_speed_string(speed_str: &str) -> Option<UsbSpeed> {
    let lower = speed_str.to_lowercase();

    // Parse "Up to X Gb/s" or "Up to X Mb/s"
    if lower.contains("gb/s") || lower.contains("gbps") {
        // Extract the number
        let num_str: String = lower.chars().filter(|c| c.is_ascii_digit()).collect();
        if let Ok(gbps) = num_str.parse::<u32>() {
            let mbps = gbps * 1000;
            return Some(UsbSpeed::from_mbps(mbps));
        }
    } else if lower.contains("mb/s") || lower.contains("mbps") {
        let num_str: String = lower.chars().filter(|c| c.is_ascii_digit()).collect();
        if let Ok(mbps) = num_str.parse::<u32>() {
            return Some(UsbSpeed::from_mbps(mbps));
        }
    }

    // Try common patterns
    if lower.contains("superspeed+") || lower.contains("20 g") {
        Some(UsbSpeed::SuperSpeedPlus20)
    } else if lower.contains("superspeed") && lower.contains("10") {
        Some(UsbSpeed::SuperSpeedPlus)
    } else if lower.contains("superspeed") || lower.contains("5 g") {
        Some(UsbSpeed::SuperSpeed)
    } else if lower.contains("high") || lower.contains("480") {
        Some(UsbSpeed::High)
    } else if lower.contains("full") || lower.contains("12") {
        Some(UsbSpeed::Full)
    } else if lower.contains("low") {
        Some(UsbSpeed::Low)
    } else {
        None
    }
}

/// Check if this is a system drive
fn check_if_system_drive(
    info: &HashMap<String, String>,
    mount_points: &[String],
    internal: bool,
) -> (bool, Option<String>) {
    if info
        .get("SystemImage")
        .map(|s| s == "true")
        .unwrap_or(false)
    {
        return (true, Some("System image volume".to_string()));
    }

    if info.get("BooterDevicePathStr").is_some() {
        return (true, Some("Boot device".to_string()));
    }

    for mp in mount_points {
        if is_system_mount_point(mp) {
            return (true, Some(format!("Contains system mount point: {}", mp)));
        }
    }

    let removable = info
        .get("RemovableMedia")
        .map(|s| s == "true")
        .unwrap_or(false);
    let ejectable = info.get("Ejectable").map(|s| s == "true").unwrap_or(false);

    if internal && !removable && !ejectable {
        return (true, Some("Internal non-removable drive".to_string()));
    }

    (false, None)
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // parse_disk_list tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_parse_disk_list_whole_disks() {
        let plist = r#"
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>AllDisks</key>
    <array>
        <string>disk0</string>
        <string>disk0s1</string>
        <string>disk0s2</string>
        <string>disk1</string>
        <string>disk1s1</string>
    </array>
    <key>WholeDisks</key>
    <array>
        <string>disk0</string>
        <string>disk1</string>
    </array>
</dict>
</plist>
        "#;

        let disks = parse_disk_list(plist).unwrap();
        assert_eq!(disks, vec!["disk0", "disk1"]);
    }

    #[test]
    fn test_parse_disk_list_empty() {
        let plist = r#"
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>WholeDisks</key>
    <array>
    </array>
</dict>
</plist>
        "#;

        let result = parse_disk_list(plist);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_disk_list_multiple() {
        let plist = r#"
<plist version="1.0">
<dict>
    <key>WholeDisks</key>
    <array>
        <string>disk0</string>
        <string>disk1</string>
        <string>disk2</string>
        <string>disk3</string>
    </array>
</dict>
</plist>
        "#;

        let disks = parse_disk_list(plist).unwrap();
        assert_eq!(disks.len(), 4);
        assert!(disks.contains(&"disk2".to_string()));
    }

    // -------------------------------------------------------------------------
    // parse_disk_info tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_parse_disk_info_basic() {
        let plist = r#"
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>DeviceNode</key>
    <string>/dev/disk2</string>
    <key>TotalSize</key>
    <integer>32000000000</integer>
    <key>RemovableMedia</key>
    <true/>
    <key>Internal</key>
    <false/>
    <key>MediaName</key>
    <string>SanDisk Ultra</string>
</dict>
</plist>
        "#;

        let info = parse_disk_info(plist).unwrap();
        assert_eq!(info.get("DeviceNode"), Some(&"/dev/disk2".to_string()));
        assert_eq!(info.get("TotalSize"), Some(&"32000000000".to_string()));
        assert_eq!(info.get("RemovableMedia"), Some(&"true".to_string()));
        assert_eq!(info.get("Internal"), Some(&"false".to_string()));
        assert_eq!(info.get("MediaName"), Some(&"SanDisk Ultra".to_string()));
    }

    #[test]
    fn test_parse_disk_info_false_values() {
        let plist = r#"
<dict>
    <key>Ejectable</key>
    <false/>
    <key>Removable</key>
    <false/>
</dict>
        "#;

        let info = parse_disk_info(plist).unwrap();
        assert_eq!(info.get("Ejectable"), Some(&"false".to_string()));
        assert_eq!(info.get("Removable"), Some(&"false".to_string()));
    }

    #[test]
    fn test_parse_disk_info_empty() {
        let plist = r#"
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
</dict>
</plist>
        "#;

        let info = parse_disk_info(plist).unwrap();
        assert!(info.is_empty());
    }

    #[test]
    fn test_parse_disk_info_special_characters() {
        let plist = r#"
<dict>
    <key>VolumeName</key>
    <string>My USB & Drive</string>
</dict>
        "#;

        let info = parse_disk_info(plist).unwrap();
        assert_eq!(info.get("VolumeName"), Some(&"My USB & Drive".to_string()));
    }

    // -------------------------------------------------------------------------
    // detect_drive_type tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_detect_drive_type_usb() {
        let mut info = HashMap::new();
        info.insert("DeviceProtocol".to_string(), "USB".to_string());
        assert_eq!(detect_drive_type(&info), DriveType::Usb);

        let mut info = HashMap::new();
        info.insert("BusProtocol".to_string(), "USB".to_string());
        assert_eq!(detect_drive_type(&info), DriveType::Usb);
    }

    #[test]
    fn test_detect_drive_type_nvme() {
        let mut info = HashMap::new();
        info.insert("DeviceProtocol".to_string(), "NVMe".to_string());
        assert_eq!(detect_drive_type(&info), DriveType::Nvme);

        let mut info = HashMap::new();
        info.insert("DeviceProtocol".to_string(), "PCI-Express".to_string());
        assert_eq!(detect_drive_type(&info), DriveType::Nvme);
    }

    #[test]
    fn test_detect_drive_type_sata() {
        let mut info = HashMap::new();
        info.insert("DeviceProtocol".to_string(), "SATA".to_string());
        assert_eq!(detect_drive_type(&info), DriveType::Sata);
    }

    #[test]
    fn test_detect_drive_type_sd_card() {
        let mut info = HashMap::new();
        info.insert("DeviceProtocol".to_string(), "Secure Digital".to_string());
        assert_eq!(detect_drive_type(&info), DriveType::SdCard);

        let mut info = HashMap::new();
        info.insert("MediaName".to_string(), "SD Card Reader".to_string());
        assert_eq!(detect_drive_type(&info), DriveType::SdCard);
    }

    #[test]
    fn test_detect_drive_type_unknown() {
        let info = HashMap::new();
        assert_eq!(detect_drive_type(&info), DriveType::Other);
    }

    // -------------------------------------------------------------------------
    // parse_partitions tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_parse_partitions_basic() {
        let plist = r#"
<plist version="1.0">
<dict>
    <key>AllDisksAndPartitions</key>
    <array>
        <dict>
            <key>DeviceIdentifier</key>
            <string>disk2</string>
            <key>Size</key>
            <integer>32000000000</integer>
        </dict>
        <dict>
            <key>DeviceIdentifier</key>
            <string>disk2s1</string>
            <key>Size</key>
            <integer>31999000000</integer>
            <key>VolumeName</key>
            <string>UBUNTU</string>
            <key>Content</key>
            <string>Microsoft Basic Data</string>
            <key>MountPoint</key>
            <string>/Volumes/UBUNTU</string>
        </dict>
    </array>
</dict>
</plist>
        "#;

        let partitions = parse_partitions(plist, "disk2").unwrap();
        assert_eq!(partitions.len(), 1);
        assert_eq!(partitions[0].path, "/dev/disk2s1");
        assert_eq!(partitions[0].label, Some("UBUNTU".to_string()));
        assert_eq!(
            partitions[0].mount_point,
            Some("/Volumes/UBUNTU".to_string())
        );
    }

    #[test]
    fn test_parse_partitions_empty() {
        let plist = r#"
<plist version="1.0">
<dict>
</dict>
</plist>
        "#;

        let partitions = parse_partitions(plist, "disk2").unwrap();
        assert!(partitions.is_empty());
    }

    // -------------------------------------------------------------------------
    // Integration tests (require actual system)
    // -------------------------------------------------------------------------

    #[test]
    #[ignore] // Run with: cargo test -- --ignored
    fn test_list_drives_real() {
        let drives = list_drives();
        assert!(drives.is_ok(), "Should be able to list drives");

        let drives = drives.unwrap();
        // On a Mac, there should be at least one drive (the system drive)
        assert!(!drives.is_empty(), "Should find at least one drive");

        // System drive should be present and marked as system
        let system_drive = drives.iter().find(|d| d.is_system);
        assert!(system_drive.is_some(), "Should identify a system drive");
    }

    // -------------------------------------------------------------------------
    // USB speed parsing tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_parse_speed_string_gbps() {
        assert_eq!(
            parse_speed_string("Up to 5 Gb/s"),
            Some(UsbSpeed::SuperSpeed)
        );
        assert_eq!(
            parse_speed_string("Up to 10 Gb/s"),
            Some(UsbSpeed::SuperSpeedPlus)
        );
        assert_eq!(
            parse_speed_string("Up to 20 Gb/s"),
            Some(UsbSpeed::SuperSpeedPlus20)
        );
    }

    #[test]
    fn test_parse_speed_string_mbps() {
        assert_eq!(parse_speed_string("Up to 480 Mb/s"), Some(UsbSpeed::High));
        assert_eq!(parse_speed_string("Up to 12 Mb/s"), Some(UsbSpeed::Full));
    }

    #[test]
    fn test_parse_speed_string_keywords() {
        assert_eq!(parse_speed_string("High Speed"), Some(UsbSpeed::High));
        assert_eq!(parse_speed_string("Full Speed"), Some(UsbSpeed::Full));
        assert_eq!(parse_speed_string("Low Speed"), Some(UsbSpeed::Low));
        assert_eq!(parse_speed_string("SuperSpeed"), Some(UsbSpeed::SuperSpeed));
    }

    #[test]
    fn test_parse_usb_speed_from_profiler() {
        let output = r#"
USB:

    USB 3.1 Bus:

      Host Controller Driver: AppleUSBXHCIPPT

        SanDisk Ultra:

          Product ID: 0x5591
          Vendor ID: 0x0781
          Speed: Up to 5 Gb/s
          Manufacturer: SanDisk
          Serial Number: ABC123

        Kingston DataTraveler:

          Product ID: 0x1666
          Speed: Up to 480 Mb/s

    USB 2.0 Bus:

      Host Controller Driver: AppleUSBEHCI
"#;

        // Should find SanDisk at SuperSpeed
        let speed = parse_usb_speed_from_profiler(output, "disk2", Some("SanDisk Ultra"));
        assert_eq!(speed, Some(UsbSpeed::SuperSpeed));

        // Should find Kingston at High Speed
        let speed = parse_usb_speed_from_profiler(output, "disk3", Some("Kingston DataTraveler"));
        assert_eq!(speed, Some(UsbSpeed::High));

        // Should not find non-existent device
        let speed = parse_usb_speed_from_profiler(output, "disk4", Some("NonExistent"));
        assert_eq!(speed, None);

        // Should return None if no media name
        let speed = parse_usb_speed_from_profiler(output, "disk2", None);
        assert_eq!(speed, None);
    }
}
