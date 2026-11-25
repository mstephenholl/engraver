//! macOS drive detection implementation
//!
//! Uses `diskutil` command for device enumeration and information.

use super::{is_system_mount_point, DetectError, Drive, DriveType, Partition, Result, UsbSpeed};
use std::collections::HashMap;
use std::process::Command;

/// List all drives on macOS
///
/// # Errors
///
/// Returns an error if `diskutil` fails to execute or produces unparseable output.
pub fn list_drives() -> Result<Vec<Drive>> {
    let output = Command::new("diskutil")
        .args(["list", "-plist"])
        .output()
        .map_err(|e| DetectError::CommandFailed {
            message: "diskutil list failed".to_string(),
            source: Some(e),
        })?;

    if !output.status.success() {
        return Err(DetectError::CommandFailed {
            message: format!(
                "diskutil list failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
            source: None,
        });
    }

    let plist_str = String::from_utf8_lossy(&output.stdout);
    let disk_names = parse_disk_list(&plist_str)?;

    let mut drives = Vec::new();

    for disk_name in disk_names {
        match get_disk_info(&disk_name) {
            Ok(Some(drive)) => drives.push(drive),
            Ok(None) => {}
            Err(e) => {
                tracing::debug!("Failed to get info for {}: {}", disk_name, e);
            }
        }
    }

    if let Some(root_info) = root_volume_info() {
        mark_apfs_boot_disks(&mut drives, &root_info);
    }

    Ok(drives)
}

/// Reason given to a disk that holds the APFS container macOS booted from
const APFS_BOOT_DISK_REASON: &str = "Holds the APFS container macOS booted from";

/// Run `diskutil info -plist /` and return its output
///
/// `/` is an APFS volume inside a synthesized container disk, which
/// `get_disk_info` skips. The volume's info is the only place that names
/// the physical disk behind it.
fn root_volume_info() -> Option<String> {
    let output = Command::new("diskutil")
        .args(["info", "-plist", "/"])
        .output()
        .inspect_err(|e| tracing::debug!("Failed to run diskutil info /: {e}"))
        .ok()?;

    if !output.status.success() {
        tracing::debug!(
            "diskutil info / failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return None;
    }

    Some(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Physical stores (such as `disk4s2`) of the APFS container that holds a volume
///
/// Takes `diskutil info -plist <volume>` output. A volume that is not APFS has
/// no stores. A Fusion drive has one on each of its two disks.
pub(crate) fn parse_apfs_physical_stores(_plist: &str) -> Vec<String> {
    Vec::new()
}

/// Whole disk that a BSD identifier lives on: `disk4s2` and `disk4` give `disk4`
///
/// Returns `None` for anything else, such as `disk3s1s1`, which names an APFS
/// volume snapshot and not a physical store.
pub(crate) fn whole_disk_of_store(store: &str) -> Option<&str> {
    let rest = store.strip_prefix("disk")?;
    let (disk, partition) = match rest.split_once('s') {
        Some((disk, partition)) => (disk, Some(partition)),
        None => (rest, None),
    };

    let is_number = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !is_number(disk) || partition.is_some_and(|p| !is_number(p)) {
        return None;
    }

    Some(&store[.."disk".len() + disk.len()])
}

/// Mark the disks behind the root volume's APFS container as system disks
///
/// Takes `diskutil info -plist /` output. macOS can boot from an external
/// APFS disk. Its volumes, mounted at `/`, sit in a container disk that
/// `get_disk_info` skips, and the physical disk has only an unmounted
/// `Apple_APFS` partition, so the mount point check misses it.
pub(crate) fn mark_apfs_boot_disks(drives: &mut [Drive], root_info: &str) {
    let stores = parse_apfs_physical_stores(root_info);
    if stores.is_empty() {
        tracing::debug!("/ has no APFS physical stores, so only mount points mark system disks");
        return;
    }

    for store in &stores {
        let Some(disk) = whole_disk_of_store(store) else {
            tracing::debug!("Ignoring APFS physical store {store}: not a disk identifier");
            continue;
        };

        let path = format!("/dev/{disk}");
        for drive in drives.iter_mut().filter(|d| d.path == path) {
            drive.is_system = true;
            drive
                .system_reason
                .get_or_insert_with(|| APFS_BOOT_DISK_REASON.to_string());
        }
    }
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
        .map_err(|e| DetectError::CommandFailed {
            message: "diskutil info failed".to_string(),
            source: Some(e),
        })?;

    if !output.status.success() {
        return Ok(None);
    }

    let plist_str = String::from_utf8_lossy(&output.stdout);
    let info = parse_disk_info(&plist_str);

    // Skip virtual/synthesized disks
    if info
        .get("VirtualOrPhysical")
        .map(std::string::String::as_str)
        == Some("Virtual")
    {
        return Ok(None);
    }

    // Skip APFS containers
    if info.contains_key("APFSContainerReference") && !info.contains_key("DeviceNode") {
        return Ok(None);
    }

    let device_node = match info.get("DeviceNode") {
        Some(node) => node.clone(),
        None => return Ok(None),
    };

    let size = info
        .get("TotalSize")
        .or_else(|| info.get("Size"))
        .and_then(|s| {
            s.parse::<u64>()
                .inspect_err(|e| tracing::warn!("Failed to parse size for {disk_name}: {e}"))
                .ok()
        })
        .unwrap_or(0);

    if size == 0 {
        tracing::debug!("Skipping disk {disk_name}: size is 0");
        return Ok(None);
    }

    let removable = info.get("RemovableMedia").is_some_and(|s| s == "true")
        || info.get("Ejectable").is_some_and(|s| s == "true");

    let internal = info.get("Internal").is_none_or(|s| s == "true");

    let vendor = info.get("MediaName").cloned();
    let model = info.get("IORegistryEntryName").cloned();
    let serial = info.get("DeviceSerialNumber").cloned();

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

    let raw_path = format!("/dev/r{disk_name}");

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
        serial,
        mount_points,
        partitions,
        system_reason,
        usb_speed,
    }))
}

/// Parse disk info plist into a key-value map
pub(crate) fn parse_disk_info(plist: &str) -> HashMap<String, String> {
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

    info
}

/// Get partitions for a disk
fn get_disk_partitions(disk_name: &str) -> Result<Vec<Partition>> {
    let output = Command::new("diskutil")
        .args(["list", "-plist", disk_name])
        .output()
        .map_err(|e| DetectError::CommandFailed {
            message: "diskutil list failed".to_string(),
            source: Some(e),
        })?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let plist_str = String::from_utf8_lossy(&output.stdout);
    Ok(parse_partitions(&plist_str, disk_name))
}

/// Parse partitions from diskutil list output
pub(crate) fn parse_partitions(plist: &str, disk_name: &str) -> Vec<Partition> {
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
                        path: format!("/dev/{dev_id}"),
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

    partitions
}

/// Detect drive type from disk info
pub(crate) fn detect_drive_type(info: &HashMap<String, String>) -> DriveType {
    let protocol = info.get("DeviceProtocol").map(std::string::String::as_str);
    let bus = info.get("BusProtocol").map(std::string::String::as_str);
    let media_name = info.get("MediaName").map(|s| s.to_lowercase());

    match protocol.or(bus) {
        Some("USB") => return DriveType::Usb,
        Some("NVMe" | "PCI-Express") => return DriveType::Nvme,
        Some("SATA" | "SAS") => return DriveType::Sata,
        Some("Secure Digital" | "SD") => return DriveType::SdCard,
        _ => {}
    }

    if let Some(name) = media_name {
        if name.contains("sd") || name.contains("card") {
            return DriveType::SdCard;
        }
    }

    DriveType::Other
}

/// Get USB speed for a device by querying `system_profiler`
///
/// Parses `system_profiler SPUSBDataType` output to find the speed
/// of the USB device matching the given disk name.
fn get_usb_speed_for_disk(disk_name: &str, info: &HashMap<String, String>) -> Option<UsbSpeed> {
    // Only check USB devices
    let protocol = info
        .get("DeviceProtocol")
        .or_else(|| info.get("BusProtocol"));
    if protocol.map(std::string::String::as_str) != Some("USB") {
        return None;
    }

    // Get the media name to help match in USB tree
    let media_name = info.get("MediaName").cloned();

    // Run system_profiler to get USB device info
    let output = Command::new("system_profiler")
        .args(["SPUSBDataType", "-detailLevel", "mini"])
        .output()
        .inspect_err(|e| tracing::debug!("Failed to run system_profiler: {e}"))
        .ok()?;

    if !output.status.success() {
        tracing::debug!("system_profiler returned non-zero exit code");
        return None;
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    parse_usb_speed_from_profiler(&output_str, disk_name, media_name.as_deref())
}

/// Parse USB speed from `system_profiler` output
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
        let num_str: String = lower.chars().filter(char::is_ascii_digit).collect();
        if let Ok(gbps) = num_str.parse::<u32>() {
            let mbps = gbps * 1000;
            return Some(UsbSpeed::from_mbps(mbps));
        }
    } else if lower.contains("mb/s") || lower.contains("mbps") {
        let num_str: String = lower.chars().filter(char::is_ascii_digit).collect();
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
    if info.get("SystemImage").is_some_and(|s| s == "true") {
        return (true, Some("System image volume".to_string()));
    }

    if info.get("BooterDevicePathStr").is_some() {
        return (true, Some("Boot device".to_string()));
    }

    for mp in mount_points {
        if is_system_mount_point(mp) {
            return (true, Some(format!("Contains system mount point: {mp}")));
        }
    }

    let removable = info.get("RemovableMedia").is_some_and(|s| s == "true");
    let ejectable = info.get("Ejectable").is_some_and(|s| s == "true");

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

        let info = parse_disk_info(plist);
        assert_eq!(info.get("DeviceNode"), Some(&"/dev/disk2".to_string()));
        assert_eq!(info.get("TotalSize"), Some(&"32000000000".to_string()));
        assert_eq!(info.get("RemovableMedia"), Some(&"true".to_string()));
        assert_eq!(info.get("Internal"), Some(&"false".to_string()));
        assert_eq!(info.get("MediaName"), Some(&"SanDisk Ultra".to_string()));
    }

    #[test]
    fn test_parse_disk_info_with_serial() {
        let plist = r"
<dict>
    <key>DeviceNode</key>
    <string>/dev/disk2</string>
    <key>DeviceSerialNumber</key>
    <string>ABC123456789</string>
    <key>MediaName</key>
    <string>SanDisk Ultra</string>
</dict>
        ";

        let info = parse_disk_info(plist);
        assert_eq!(
            info.get("DeviceSerialNumber"),
            Some(&"ABC123456789".to_string())
        );
    }

    #[test]
    fn test_parse_disk_info_false_values() {
        let plist = r"
<dict>
    <key>Ejectable</key>
    <false/>
    <key>Removable</key>
    <false/>
</dict>
        ";

        let info = parse_disk_info(plist);
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

        let info = parse_disk_info(plist);
        assert!(info.is_empty());
    }

    #[test]
    fn test_parse_disk_info_special_characters() {
        let plist = r"
<dict>
    <key>VolumeName</key>
    <string>My USB & Drive</string>
</dict>
        ";

        let info = parse_disk_info(plist);
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

        let partitions = parse_partitions(plist, "disk2");
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

        let partitions = parse_partitions(plist, "disk2");
        assert!(partitions.is_empty());
    }

    // -------------------------------------------------------------------------
    // Integration tests (require actual system)
    // -------------------------------------------------------------------------

    #[test]
    #[ignore = "requires real hardware; run with: cargo test -- --ignored"]
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
        let output = r"
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
";

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

    // -------------------------------------------------------------------------
    // APFS boot disk tests
    // -------------------------------------------------------------------------

    /// `diskutil info -plist /` for a Mac booted from APFS on `disk4s2`. The
    /// volume is `disk5s1s1`, in the synthesized container `disk5`.
    const ROOT_INFO_DISK4: &str = r#"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>APFSContainerReference</key>
    <string>disk5</string>
    <key>APFSPhysicalStores</key>
    <array>
        <dict>
            <key>APFSPhysicalStore</key>
            <string>disk4s2</string>
        </dict>
    </array>
    <key>DeviceIdentifier</key>
    <string>disk5s1s1</string>
    <key>FilesystemType</key>
    <string>apfs</string>
    <key>MountPoint</key>
    <string>/</string>
    <key>ParentWholeDisk</key>
    <string>disk5</string>
    <key>VolumeName</key>
    <string>Macintosh HD</string>
</dict>
</plist>
"#;

    /// `diskutil info -plist /` for a Fusion drive, whose APFS container
    /// spans `disk0s2` and `disk1s2`
    const ROOT_INFO_FUSION: &str = r#"
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>APFSContainerReference</key>
    <string>disk2</string>
    <key>APFSPhysicalStores</key>
    <array>
        <dict>
            <key>APFSPhysicalStore</key>
            <string>disk0s2</string>
        </dict>
        <dict>
            <key>APFSPhysicalStore</key>
            <string>disk1s2</string>
        </dict>
    </array>
    <key>DeviceIdentifier</key>
    <string>disk2s1</string>
    <key>MountPoint</key>
    <string>/</string>
</dict>
</plist>
"#;

    /// `diskutil info -plist /` for an HFS+ root, which has no APFS keys
    const ROOT_INFO_HFS: &str = r#"
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>DeviceIdentifier</key>
    <string>disk1s2</string>
    <key>FilesystemType</key>
    <string>hfs</string>
    <key>MountPoint</key>
    <string>/</string>
</dict>
</plist>
"#;

    /// `diskutil info -plist <disk>` for an external, ejectable whole disk
    fn external_disk_info(disk: &str) -> String {
        format!(
            r#"
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
    <key>DeviceNode</key>
    <string>/dev/{disk}</string>
    <key>Ejectable</key>
    <true/>
    <key>Internal</key>
    <false/>
    <key>MediaName</key>
    <string>Portable SSD</string>
    <key>RemovableMedia</key>
    <false/>
    <key>TotalSize</key>
    <integer>1000204886016</integer>
    <key>VirtualOrPhysical</key>
    <string>Physical</string>
</dict>
</plist>
"#
        )
    }

    /// Build a drive the way `get_disk_info` does, from `diskutil info` output
    /// and no mount points
    fn unmounted_drive(info_plist: &str) -> Drive {
        let info = parse_disk_info(info_plist);
        let internal = info.get("Internal").is_none_or(|s| s == "true");
        let (is_system, system_reason) = check_if_system_drive(&info, &[], internal);

        Drive {
            is_system,
            system_reason,
            ..Drive::new(info["DeviceNode"].clone())
        }
    }

    #[test]
    fn test_external_boot_disk_is_marked_as_system_disk() {
        let mut drives = vec![
            unmounted_drive(&external_disk_info("disk4")),
            unmounted_drive(&external_disk_info("disk6")),
        ];
        assert!(
            !drives[0].is_system,
            "the boot disk has no mount point to give it away"
        );

        mark_apfs_boot_disks(&mut drives, ROOT_INFO_DISK4);

        assert!(drives[0].is_system, "disk4 holds the container for /");
        assert!(drives[0]
            .system_reason
            .as_deref()
            .is_some_and(|r| r.contains("macOS booted from")));
        assert!(!drives[1].is_system, "disk6 has nothing to do with /");
    }

    #[test]
    fn test_fusion_boot_disks_are_all_marked_as_system_disks() {
        let mut drives = vec![
            Drive::new("/dev/disk0"),
            Drive::new("/dev/disk1"),
            Drive::new("/dev/disk3"),
        ];

        mark_apfs_boot_disks(&mut drives, ROOT_INFO_FUSION);

        assert!(drives[0].is_system, "disk0 holds the first store");
        assert!(drives[1].is_system, "disk1 holds the second store");
        assert!(!drives[2].is_system, "disk3 holds neither store");
    }

    #[test]
    fn test_boot_disk_keeps_the_reason_it_already_has() {
        let mut drive = Drive::new("/dev/disk4");
        drive.is_system = true;
        drive.system_reason = Some("Contains system mount point: /".to_string());
        let mut drives = vec![drive];

        mark_apfs_boot_disks(&mut drives, ROOT_INFO_DISK4);

        assert!(drives[0].is_system);
        assert_eq!(
            drives[0].system_reason.as_deref(),
            Some("Contains system mount point: /")
        );
    }

    #[test]
    fn test_root_without_apfs_stores_marks_nothing() {
        let empty_stores = r"
<dict>
    <key>APFSPhysicalStores</key>
    <array/>
</dict>
";

        for root_info in [ROOT_INFO_HFS, empty_stores, ""] {
            let mut drives = vec![Drive::new("/dev/disk1")];
            mark_apfs_boot_disks(&mut drives, root_info);
            assert!(!drives[0].is_system, "marked by {root_info:?}");
            assert!(drives[0].system_reason.is_none());
        }
    }

    #[test]
    fn test_whole_disk_of_store() {
        assert_eq!(whole_disk_of_store("disk4s2"), Some("disk4"));
        assert_eq!(whole_disk_of_store("disk12s34"), Some("disk12"));
        assert_eq!(whole_disk_of_store("disk4"), Some("disk4"));
    }

    #[test]
    fn test_whole_disk_of_store_rejects_other_identifiers() {
        for not_a_store in [
            "",
            "disk",
            "disks2",
            "disk4s",
            "disk4s2s1", // a volume snapshot
            "disk4x2",
            "diskas2",
            "/dev/disk4s2",
            "rdisk4s2",
        ] {
            assert_eq!(whole_disk_of_store(not_a_store), None, "{not_a_store:?}");
        }
    }

    // The next test runs the real `diskutil`, so it checks the key names this
    // module reads against what macOS emits. It assumes `/` is an APFS volume,
    // as it is on GitHub's macOS runners.
    #[test]
    fn test_real_root_volume_stores_are_listed_disks() {
        let root_info = root_volume_info().expect("diskutil info -plist / should succeed");
        let stores = parse_apfs_physical_stores(&root_info);
        assert!(
            !stores.is_empty(),
            "no APFS physical stores found in:\n{root_info}"
        );

        let list = Command::new("diskutil")
            .args(["list", "-plist"])
            .output()
            .expect("diskutil list -plist should run");
        let whole_disks = parse_disk_list(&String::from_utf8_lossy(&list.stdout))
            .expect("diskutil list -plist should name disks");

        for store in &stores {
            let disk = whole_disk_of_store(store)
                .unwrap_or_else(|| panic!("{store:?} is not a disk identifier"));
            assert!(
                whole_disks.iter().any(|d| d == disk),
                "{disk} (from {store}) is not in WholeDisks: {whole_disks:?}"
            );
        }
    }
}
