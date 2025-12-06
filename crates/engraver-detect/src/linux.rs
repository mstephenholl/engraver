//! Linux drive detection implementation
//!
//! Uses /sys/block for device enumeration and /proc/mounts for mount point detection.

use super::{is_system_mount_point, DetectError, Drive, DriveType, Partition, Result};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// List all drives on Linux
pub fn list_drives() -> Result<Vec<Drive>> {
    let mut drives = Vec::new();
    let mut mount_map = get_mount_points()?;

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

        if let Some(drive) = parse_block_device(&name, &mut mount_map)? {
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
        || name.starts_with("fd")  // Floppy drives
}

/// Parse a block device from /sys/block
fn parse_block_device(
    name: &str,
    mount_map: &mut HashMap<String, String>,
) -> Result<Option<Drive>> {
    let sys_path = format!("/sys/block/{}", name);
    let dev_path = format!("/dev/{}", name);

    if !Path::new(&sys_path).exists() {
        return Ok(None);
    }

    // Get size (in 512-byte sectors)
    let size = read_sys_value(&format!("{}/size", sys_path))
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .map(|sectors| sectors * 512)
        .unwrap_or(0);

    if size == 0 {
        return Ok(None);
    }

    let removable = read_sys_value(&format!("{}/removable", sys_path))
        .map(|s| s.trim() == "1")
        .unwrap_or(false);

    let vendor = read_sys_value(&format!("{}/device/vendor", sys_path))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let model = read_sys_value(&format!("{}/device/model", sys_path))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    let drive_type = detect_drive_type(name, &sys_path);
    let partitions = get_partitions(name, mount_map)?;

    let mount_points: Vec<String> = partitions
        .iter()
        .filter_map(|p| p.mount_point.clone())
        .collect();

    let (is_system, system_reason) = check_if_system_drive(name, &mount_points, removable);

    let display_name = match (&vendor, &model) {
        (Some(v), Some(m)) => format!("{} {}", v, m),
        (None, Some(m)) => m.clone(),
        (Some(v), None) => v.clone(),
        (None, None) => name.to_string(),
    };

    Ok(Some(Drive {
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
    }))
}

/// Get mount points from /proc/mounts
pub(crate) fn get_mount_points() -> Result<HashMap<String, String>> {
    let mut mounts = HashMap::new();

    let content = fs::read_to_string("/proc/mounts").map_err(|e| {
        DetectError::EnumerationFailed(format!("Failed to read /proc/mounts: {}", e))
    })?;

    for line in content.lines() {
        if let Some((device, mount_point)) = parse_mount_line(line) {
            mounts.insert(device, mount_point);
        }
    }

    Ok(mounts)
}

/// Parse a single line from /proc/mounts
pub(crate) fn parse_mount_line(line: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 2 {
        Some((parts[0].to_string(), parts[1].to_string()))
    } else {
        None
    }
}

/// Get partitions for a device
fn get_partitions(
    device_name: &str,
    mount_map: &mut HashMap<String, String>,
) -> Result<Vec<Partition>> {
    let mut partitions = Vec::new();
    let sys_path = format!("/sys/block/{}", device_name);

    if let Ok(entries) = fs::read_dir(&sys_path) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();

            if name.starts_with(device_name) && name != device_name {
                let part_path = format!("/dev/{}", name);
                let part_sys_path = format!("{}/{}", sys_path, name);

                let size = read_sys_value(&format!("{}/size", part_sys_path))
                    .ok()
                    .and_then(|s| s.parse::<u64>().ok())
                    .map(|sectors| sectors * 512)
                    .unwrap_or(0);

                let mount_point = mount_map.get(&part_path).cloned();

                partitions.push(Partition {
                    path: part_path,
                    label: None,
                    filesystem: None,
                    size,
                    mount_point,
                });
            }
        }
    }

    partitions.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(partitions)
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
    if let Ok(subsystem_link) = fs::read_link(format!("{}/device/subsystem", sys_path)) {
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
    let removable = read_sys_value(&format!("{}/removable", sys_path))
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
            return (
                true,
                Some(format!("Contains system mount point: {}", mp)),
            );
        }
    }

    // Non-removable drives are likely system drives
    // Exception: some external NVMe drives report as non-removable
    if !removable && !name.starts_with("nvme") {
        return (
            true,
            Some("Non-removable internal drive".to_string()),
        );
    }

    (false, None)
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
        assert_eq!(result, Some(("/dev/sda1".to_string(), "/".to_string())));
    }

    #[test]
    fn test_parse_mount_line_with_spaces() {
        let line = "/dev/sdb1 /mnt/my\\040drive vfat rw 0 0";
        let result = parse_mount_line(line);
        assert_eq!(result, Some(("/dev/sdb1".to_string(), "/mnt/my\\040drive".to_string())));
    }

    #[test]
    fn test_parse_mount_line_tmpfs() {
        let line = "tmpfs /tmp tmpfs rw,nosuid,nodev 0 0";
        let result = parse_mount_line(line);
        assert_eq!(result, Some(("tmpfs".to_string(), "/tmp".to_string())));
    }

    #[test]
    fn test_parse_mount_line_empty() {
        let result = parse_mount_line("");
        assert_eq!(result, None);
    }

    #[test]
    fn test_parse_mount_line_single_field() {
        let result = parse_mount_line("/dev/sda1");
        assert_eq!(result, None);
    }

    // -------------------------------------------------------------------------
    // detect_drive_type tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_detect_drive_type_nvme() {
        assert_eq!(detect_drive_type("nvme0n1", "/sys/block/nvme0n1"), DriveType::Nvme);
        assert_eq!(detect_drive_type("nvme1n1", "/sys/block/nvme1n1"), DriveType::Nvme);
    }

    #[test]
    fn test_detect_drive_type_sd_card() {
        assert_eq!(detect_drive_type("mmcblk0", "/sys/block/mmcblk0"), DriveType::SdCard);
        assert_eq!(detect_drive_type("mmcblk1", "/sys/block/mmcblk1"), DriveType::SdCard);
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
        let (is_system, reason) = check_if_system_drive("sda", &["/boot".to_string()], false);
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
    #[ignore] // Run with: cargo test -- --ignored
    fn test_get_mount_points_real() {
        let mounts = get_mount_points();
        assert!(mounts.is_ok(), "Should be able to read /proc/mounts");
        
        let mounts = mounts.unwrap();
        // There should be at least the root mount
        assert!(
            mounts.values().any(|mp| mp == "/"),
            "Should find root mount point"
        );
    }

    #[test]
    #[ignore]
    fn test_list_drives_real() {
        let drives = list_drives();
        assert!(drives.is_ok(), "Should be able to list drives");
        
        let drives = drives.unwrap();
        // On most Linux systems, there's at least one drive
        assert!(!drives.is_empty(), "Should find at least one drive");
    }
}
