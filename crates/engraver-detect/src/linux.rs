//! Linux drive detection implementation
//!
//! Uses /sys/block for device enumeration and /proc/mounts for mount point detection.

use super::{is_system_mount_point, DetectError, Drive, DriveType, Partition, Result, UsbSpeed};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Mount information for a device
#[derive(Debug, Clone)]
pub(crate) struct MountInfo {
    pub mount_point: String,
    pub filesystem: Option<String>,
}

/// List all drives on Linux
///
/// # Errors
///
/// Returns an error if:
/// - `/sys/block` directory doesn't exist or can't be read
/// - `/proc/mounts` can't be read for mount point detection
/// - Individual device entries can't be parsed
pub fn list_drives() -> Result<Vec<Drive>> {
    let mut drives = Vec::new();
    let mount_map = get_mount_info()?;
    let label_map = get_partition_labels();

    let block_dir = Path::new("/sys/block");
    if !block_dir.exists() {
        return Err(DetectError::EnumerationFailed(
            "/sys/block not found".to_string(),
        ));
    }

    for entry in fs::read_dir(block_dir)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();

        // Skip loop devices, ram disks, and device-mapper
        if should_skip_device(&name) {
            continue;
        }

        if let Some(drive) = parse_block_device(&name, &mount_map, &label_map) {
            drives.push(drive);
        }
    }

    Ok(drives)
}

/// Check if a device should be skipped
pub(crate) fn should_skip_device(name: &str) -> bool {
    name.starts_with("loop")
        || name.starts_with("ram")
        || name.starts_with("dm-")
        || name.starts_with("zram")
        || name.starts_with("sr")  // CD/DVD drives
        || name.starts_with("fd") // Floppy drives
}

/// Parse a block device from /sys/block
fn parse_block_device(
    name: &str,
    mount_map: &HashMap<String, MountInfo>,
    label_map: &HashMap<String, String>,
) -> Option<Drive> {
    let sys_path = format!("/sys/block/{name}");
    let dev_path = format!("/dev/{name}");

    if !Path::new(&sys_path).exists() {
        return None;
    }

    // Get size (in 512-byte sectors)
    let size = read_sys_value(&format!("{sys_path}/size"))
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map_or(0, |sectors| sectors * 512);

    if size == 0 {
        return None;
    }

    let removable = read_sys_value(&format!("{sys_path}/removable"))
        .map(|s| s.trim() == "1")
        .unwrap_or(false);

    let vendor = read_sys_value(&format!("{sys_path}/device/vendor"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let model = read_sys_value(&format!("{sys_path}/device/model"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let drive_type = detect_drive_type(name, &sys_path);
    let partitions = get_partitions(name, mount_map, label_map);

    let mount_points: Vec<String> = partitions
        .iter()
        .filter_map(|p| p.mount_point.clone())
        .collect();

    let (is_system, system_reason) = check_if_system_drive(name, &mount_points, removable);

    // Detect USB speed for USB drives
    let usb_speed = if drive_type == DriveType::Usb {
        detect_usb_speed(&sys_path)
    } else {
        None
    };

    let display_name = match (&vendor, &model) {
        (Some(v), Some(m)) => format!("{v} {m}"),
        (None, Some(m)) => m.clone(),
        (Some(v), None) => v.clone(),
        (None, None) => name.to_string(),
    };

    Some(Drive {
        path: dev_path.clone(),
        raw_path: dev_path,
        name: display_name,
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
    })
}

/// Get mount info from /proc/mounts (mount point and filesystem type)
///
/// # Errors
///
/// Returns an error if `/proc/mounts` cannot be read.
pub(crate) fn get_mount_info() -> Result<HashMap<String, MountInfo>> {
    let mut mounts = HashMap::new();

    let content = fs::read_to_string("/proc/mounts")
        .map_err(|e| DetectError::EnumerationFailed(format!("Failed to read /proc/mounts: {e}")))?;

    for line in content.lines() {
        if let Some((device, info)) = parse_mount_line(line) {
            mounts.insert(device, info);
        }
    }

    Ok(mounts)
}

/// Parse a single line from /proc/mounts
/// Format: device `mount_point` filesystem options dump pass
pub(crate) fn parse_mount_line(line: &str) -> Option<(String, MountInfo)> {
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

/// Get partition labels from /dev/disk/by-label/
///
/// Returns a map of device path -> label
pub(crate) fn get_partition_labels() -> HashMap<String, String> {
    let mut labels = HashMap::new();
    let label_dir = Path::new("/dev/disk/by-label");

    if let Ok(entries) = fs::read_dir(label_dir) {
        for entry in entries.flatten() {
            let label = entry.file_name().to_string_lossy().to_string();
            // Decode URL-encoded characters in label (e.g., \x20 for space)
            let label = decode_label(&label);

            if let Ok(target) = fs::read_link(entry.path()) {
                // Resolve the symlink to get the actual device path
                // Target is usually something like "../../sdb1"
                if let Some(device_name) = target.file_name() {
                    let device_path = format!("/dev/{}", device_name.to_string_lossy());
                    labels.insert(device_path, label);
                }
            }
        }
    }

    labels
}

/// Decode URL-encoded label (handles \x20 style escapes)
fn decode_label(label: &str) -> String {
    let mut result = String::new();
    let mut chars = label.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' && chars.peek() == Some(&'x') {
            chars.next(); // consume 'x'
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            } else {
                result.push('\\');
                result.push('x');
                result.push_str(&hex);
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// Get partitions for a device
fn get_partitions(
    device_name: &str,
    mount_map: &HashMap<String, MountInfo>,
    label_map: &HashMap<String, String>,
) -> Vec<Partition> {
    let mut partitions = Vec::new();
    let sys_path = format!("/sys/block/{device_name}");

    if let Ok(entries) = fs::read_dir(&sys_path) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();

            if name.starts_with(device_name) && name != device_name {
                let part_path = format!("/dev/{name}");
                let part_sys_path = format!("{sys_path}/{name}");

                let size = read_sys_value(&format!("{part_sys_path}/size"))
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
                    .map_or(0, |sectors| sectors * 512);

                // Get mount info (mount point and filesystem)
                let mount_info = mount_map.get(&part_path);
                let mount_point = mount_info.map(|m| m.mount_point.clone());
                let filesystem = mount_info.and_then(|m| m.filesystem.clone());

                // Get label from /dev/disk/by-label/
                let label = label_map.get(&part_path).cloned();

                partitions.push(Partition {
                    path: part_path,
                    label,
                    filesystem,
                    size,
                    mount_point,
                });
            }
        }
    }

    partitions.sort_by(|a, b| a.path.cmp(&b.path));
    partitions
}

/// Detect drive type from name and sysfs info
pub(crate) fn detect_drive_type(name: &str, sys_path: &str) -> DriveType {
    // NVMe devices
    if name.starts_with("nvme") {
        return DriveType::Nvme;
    }

    // SD cards via MMC subsystem
    if name.starts_with("mmcblk") {
        return DriveType::SdCard;
    }

    // Check USB via device subsystem
    if let Ok(subsystem_link) = fs::read_link(format!("{sys_path}/device/subsystem")) {
        if let Some(subsystem_name) = subsystem_link.file_name() {
            let sub = subsystem_name.to_string_lossy();
            if sub == "usb" || sub == "usb-storage" {
                return DriveType::Usb;
            }
            if sub == "ata" || sub == "scsi" {
                return DriveType::Sata;
            }
        }
    }

    // Check removable attribute as fallback for USB
    let removable = read_sys_value(&format!("{sys_path}/removable"))
        .map(|s| s.trim() == "1")
        .unwrap_or(false);

    if removable && name.starts_with("sd") {
        return DriveType::Usb;
    }

    DriveType::Other
}

/// Check if a drive is a system drive
pub(crate) fn check_if_system_drive(
    name: &str,
    mount_points: &[String],
    removable: bool,
) -> (bool, Option<String>) {
    // Check mount points for system paths
    for mp in mount_points {
        if is_system_mount_point(mp) {
            return (true, Some(format!("Contains system mount point: {mp}")));
        }
    }

    // Non-removable drives are likely system drives
    // Exception: some external NVMe drives report as non-removable
    if !removable && !name.starts_with("nvme") {
        return (true, Some("Non-removable internal drive".to_string()));
    }

    (false, None)
}

/// Detect USB connection speed for a block device
///
/// Traverses the sysfs device hierarchy upward from the block device
/// to find the USB device node, which contains the `speed` attribute.
///
/// Returns `None` for non-USB devices or if speed cannot be determined.
pub(crate) fn detect_usb_speed(sys_path: &str) -> Option<UsbSpeed> {
    // Get the real device path by following the device symlink
    let device_link = format!("{sys_path}/device");
    let device_path = fs::read_link(&device_link).ok()?;

    // Resolve to absolute path
    let sys_block = Path::new(sys_path);
    let absolute_device = sys_block.join(&device_path).canonicalize().ok()?;

    // Walk up the directory tree looking for a USB speed file
    // USB devices have a "speed" file containing the connection speed in Mbps
    let mut current = absolute_device.as_path();

    // Limit traversal depth to avoid infinite loops
    for _ in 0..15 {
        let speed_file = current.join("speed");
        if speed_file.exists() {
            if let Ok(speed_str) = fs::read_to_string(&speed_file) {
                if let Ok(mbps) = speed_str.trim().parse::<u32>() {
                    return Some(UsbSpeed::from_mbps(mbps));
                }
            }
        }

        // Move up one directory
        match current.parent() {
            Some(parent) if parent != Path::new("/sys") && parent != Path::new("/") => {
                current = parent;
            }
            _ => break,
        }
    }

    None
}

/// Read a value from sysfs
fn read_sys_value(path: &str) -> Result<String> {
    fs::read_to_string(path)
        .map(|s| s.trim().to_string())
        .map_err(DetectError::Io)
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // should_skip_device tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_should_skip_loop_devices() {
        assert!(should_skip_device("loop0"));
        assert!(should_skip_device("loop1"));
        assert!(should_skip_device("loop99"));
    }

    #[test]
    fn test_should_skip_ram_devices() {
        assert!(should_skip_device("ram0"));
        assert!(should_skip_device("ram15"));
    }

    #[test]
    fn test_should_skip_device_mapper() {
        assert!(should_skip_device("dm-0"));
        assert!(should_skip_device("dm-1"));
    }

    #[test]
    fn test_should_skip_zram() {
        assert!(should_skip_device("zram0"));
    }

    #[test]
    fn test_should_skip_optical() {
        assert!(should_skip_device("sr0"));
        assert!(should_skip_device("sr1"));
    }

    #[test]
    fn test_should_not_skip_real_devices() {
        assert!(!should_skip_device("sda"));
        assert!(!should_skip_device("sdb"));
        assert!(!should_skip_device("nvme0n1"));
        assert!(!should_skip_device("mmcblk0"));
        assert!(!should_skip_device("vda"));
    }

    // -------------------------------------------------------------------------
    // parse_mount_line tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_parse_mount_line_basic() {
        let line = "/dev/sda1 / ext4 rw,relatime 0 0";
        let result = parse_mount_line(line);
        assert!(result.is_some());
        let (device, info) = result.unwrap();
        assert_eq!(device, "/dev/sda1");
        assert_eq!(info.mount_point, "/");
        assert_eq!(info.filesystem, Some("ext4".to_string()));
    }

    #[test]
    fn test_parse_mount_line_with_spaces() {
        let line = "/dev/sdb1 /mnt/my\\040drive vfat rw 0 0";
        let result = parse_mount_line(line);
        assert!(result.is_some());
        let (device, info) = result.unwrap();
        assert_eq!(device, "/dev/sdb1");
        assert_eq!(info.mount_point, "/mnt/my\\040drive");
        assert_eq!(info.filesystem, Some("vfat".to_string()));
    }

    #[test]
    fn test_parse_mount_line_tmpfs() {
        let line = "tmpfs /tmp tmpfs rw,nosuid,nodev 0 0";
        let result = parse_mount_line(line);
        assert!(result.is_some());
        let (device, info) = result.unwrap();
        assert_eq!(device, "tmpfs");
        assert_eq!(info.mount_point, "/tmp");
        // tmpfs is filtered out as pseudo filesystem
        assert_eq!(info.filesystem, None);
    }

    #[test]
    fn test_parse_mount_line_empty() {
        let result = parse_mount_line("");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_mount_line_single_field() {
        let result = parse_mount_line("/dev/sda1");
        assert!(result.is_none());
    }

    #[test]
    fn test_parse_mount_line_two_fields() {
        // Two fields is not enough (need at least device, mount_point, filesystem)
        let result = parse_mount_line("/dev/sda1 /");
        assert!(result.is_none());
    }

    // -------------------------------------------------------------------------
    // decode_label tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_decode_label_simple() {
        assert_eq!(decode_label("UBUNTU"), "UBUNTU");
        assert_eq!(decode_label("my-drive"), "my-drive");
    }

    #[test]
    fn test_decode_label_with_space() {
        assert_eq!(decode_label("My\\x20Drive"), "My Drive");
    }

    #[test]
    fn test_decode_label_multiple_escapes() {
        assert_eq!(decode_label("A\\x20B\\x20C"), "A B C");
    }

    #[test]
    fn test_decode_label_invalid_escape() {
        // Invalid hex should be preserved
        assert_eq!(decode_label("Test\\xZZ"), "Test\\xZZ");
    }

    // -------------------------------------------------------------------------
    // detect_drive_type tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_detect_drive_type_nvme() {
        assert_eq!(
            detect_drive_type("nvme0n1", "/sys/block/nvme0n1"),
            DriveType::Nvme
        );
        assert_eq!(
            detect_drive_type("nvme1n1", "/sys/block/nvme1n1"),
            DriveType::Nvme
        );
    }

    #[test]
    fn test_detect_drive_type_sd_card() {
        assert_eq!(
            detect_drive_type("mmcblk0", "/sys/block/mmcblk0"),
            DriveType::SdCard
        );
        assert_eq!(
            detect_drive_type("mmcblk1", "/sys/block/mmcblk1"),
            DriveType::SdCard
        );
    }

    #[test]
    fn test_detect_drive_type_unknown() {
        // Without sysfs, we get Other for regular devices
        assert_eq!(detect_drive_type("sda", "/nonexistent"), DriveType::Other);
    }

    // -------------------------------------------------------------------------
    // check_if_system_drive tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_check_if_system_drive_root() {
        let (is_system, reason) = check_if_system_drive("sda", &["/".to_string()], false);
        assert!(is_system);
        assert!(reason.unwrap().contains("system mount point"));
    }

    #[test]
    fn test_check_if_system_drive_home() {
        let (is_system, reason) = check_if_system_drive("sda", &["/home".to_string()], false);
        assert!(is_system);
        assert!(reason.unwrap().contains("/home"));
    }

    #[test]
    fn test_check_if_system_drive_boot() {
        let (is_system, _reason) = check_if_system_drive("sda", &["/boot".to_string()], false);
        assert!(is_system);
    }

    #[test]
    fn test_check_if_system_drive_non_removable() {
        let (is_system, reason) = check_if_system_drive("sda", &[], false);
        assert!(is_system);
        assert!(reason.unwrap().contains("Non-removable"));
    }

    #[test]
    fn test_check_if_system_drive_removable_no_system_mounts() {
        let (is_system, reason) = check_if_system_drive("sdb", &["/mnt/usb".to_string()], true);
        assert!(!is_system);
        assert!(reason.is_none());
    }

    #[test]
    fn test_check_if_system_drive_nvme_non_removable_allowed() {
        // External NVMe drives report as non-removable but aren't system drives
        let (is_system, reason) = check_if_system_drive("nvme1n1", &[], false);
        assert!(!is_system);
        assert!(reason.is_none());
    }

    #[test]
    fn test_check_if_system_drive_removable_safe() {
        let mount_points = vec![
            "/media/user/USB".to_string(),
            "/run/media/user/disk".to_string(),
        ];
        let (is_system, _) = check_if_system_drive("sdc", &mount_points, true);
        assert!(!is_system);
    }

    // -------------------------------------------------------------------------
    // Integration tests (require actual Linux system)
    // -------------------------------------------------------------------------

    #[test]
    #[ignore = "requires actual Linux system, run with: cargo test -- --ignored"]
    fn test_get_mount_info_real() {
        let mounts = get_mount_info();
        assert!(mounts.is_ok(), "Should be able to read /proc/mounts");

        let mounts = mounts.unwrap();
        // There should be at least the root mount
        assert!(
            mounts.values().any(|info| info.mount_point == "/"),
            "Should find root mount point"
        );
        // Root should have a filesystem
        let root_info = mounts.values().find(|info| info.mount_point == "/");
        assert!(root_info.is_some());
        assert!(root_info.unwrap().filesystem.is_some());
    }

    #[test]
    #[ignore = "requires actual Linux system, run with: cargo test -- --ignored"]
    fn test_get_partition_labels_real() {
        // This test just verifies the function doesn't panic
        // Labels may or may not exist on the system
        let labels = get_partition_labels();
        // If there are labels, they should map to valid device paths
        for (device, label) in &labels {
            assert!(
                device.starts_with("/dev/"),
                "Device should start with /dev/"
            );
            assert!(!label.is_empty(), "Label should not be empty");
        }
    }

    #[test]
    #[ignore = "requires actual Linux system, run with: cargo test -- --ignored"]
    fn test_list_drives_real() {
        let drives = list_drives();
        assert!(drives.is_ok(), "Should be able to list drives");

        let drives = drives.unwrap();
        // On most Linux systems, there's at least one drive
        assert!(!drives.is_empty(), "Should find at least one drive");
    }

    // -------------------------------------------------------------------------
    // USB speed detection tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_usb_speed_from_mbps() {
        assert_eq!(UsbSpeed::from_mbps(1), UsbSpeed::Low);
        assert_eq!(UsbSpeed::from_mbps(12), UsbSpeed::Full);
        assert_eq!(UsbSpeed::from_mbps(480), UsbSpeed::High);
        assert_eq!(UsbSpeed::from_mbps(5000), UsbSpeed::SuperSpeed);
        assert_eq!(UsbSpeed::from_mbps(10000), UsbSpeed::SuperSpeedPlus);
        assert_eq!(UsbSpeed::from_mbps(20000), UsbSpeed::SuperSpeedPlus20);
    }

    #[test]
    fn test_usb_speed_is_slow() {
        assert!(UsbSpeed::Low.is_slow());
        assert!(UsbSpeed::Full.is_slow());
        assert!(UsbSpeed::High.is_slow());
        assert!(!UsbSpeed::SuperSpeed.is_slow());
        assert!(!UsbSpeed::SuperSpeedPlus.is_slow());
        assert!(!UsbSpeed::SuperSpeedPlus20.is_slow());
        assert!(!UsbSpeed::Unknown.is_slow());
    }

    #[test]
    fn test_usb_speed_display() {
        assert_eq!(UsbSpeed::High.to_string(), "USB 2.0 (480 Mbps)");
        assert_eq!(UsbSpeed::SuperSpeed.to_string(), "USB 3.0 (5 Gbps)");
    }

    #[test]
    fn test_detect_usb_speed_nonexistent() {
        // Non-existent path should return None
        assert!(detect_usb_speed("/sys/block/nonexistent").is_none());
    }
}
