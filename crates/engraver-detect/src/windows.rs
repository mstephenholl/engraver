//! Windows drive detection implementation
//!
//! Uses `wmic` and WMI queries for device enumeration.

use super::{is_system_mount_point, DetectError, Drive, DriveType, Partition, Result};
use std::collections::HashMap;
use std::process::Command;

/// List all drives on Windows
pub fn list_drives() -> Result<Vec<Drive>> {
    let disks = get_physical_disks()?;
    let volumes = get_volumes()?;

    let mut drives = Vec::new();

    for disk in disks {
        let partitions = get_disk_partitions(&disk.device_id, &volumes);
        let mount_points: Vec<String> = partitions
            .iter()
            .filter_map(|p| p.mount_point.clone())
            .collect();

        let (is_system, system_reason) =
            check_if_system_drive(&disk.media_type, &mount_points, &disk.interface_type);

        let raw_path = format!("\\\\.\\PhysicalDrive{}", disk.index);

        let removable = disk.media_type.contains("Removable") 
            || disk.media_type.contains("External");

        drives.push(Drive {
            path: raw_path.clone(),
            raw_path,
            name: disk.model.clone(),
            size: disk.size,
            removable,
            is_system,
            drive_type: detect_drive_type(&disk.interface_type, &disk.media_type),
            vendor: None,
            model: Some(disk.model),
            serial: disk.serial,
            mount_points,
            partitions,
            system_reason,
        });
    }

    Ok(drives)
}

/// Physical disk info from WMI
#[derive(Debug, Clone)]
pub(crate) struct PhysicalDisk {
    pub index: u32,
    pub device_id: String,
    pub model: String,
    pub size: u64,
    pub media_type: String,
    pub interface_type: String,
    pub serial: Option<String>,
}

/// Get physical disks using wmic
fn get_physical_disks() -> Result<Vec<PhysicalDisk>> {
    let output = Command::new("wmic")
        .args([
            "diskdrive",
            "get",
            "Index,DeviceID,Model,Size,MediaType,InterfaceType,SerialNumber",
            "/format:csv",
        ])
        .output()
        .map_err(|e| DetectError::CommandFailed(format!("wmic failed: {}", e)))?;

    if !output.status.success() {
        return Err(DetectError::CommandFailed(format!(
            "wmic failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    parse_wmic_disks(&output_str)
}

/// Parse wmic diskdrive CSV output
pub(crate) fn parse_wmic_disks(csv: &str) -> Result<Vec<PhysicalDisk>> {
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

        if size == 0 {
            continue;
        }

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

    Ok(disks)
}

/// Volume info
#[derive(Debug, Clone)]
pub(crate) struct VolumeInfo {
    pub drive_letter: String,
    pub label: Option<String>,
    pub filesystem: Option<String>,
    pub size: u64,
}

/// Get volumes using wmic
fn get_volumes() -> Result<Vec<VolumeInfo>> {
    let output = Command::new("wmic")
        .args([
            "volume",
            "get",
            "DriveLetter,Label,FileSystem,Capacity",
            "/format:csv",
        ])
        .output()
        .map_err(|e| DetectError::CommandFailed(format!("wmic volume failed: {}", e)))?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    Ok(parse_wmic_volumes(&output_str))
}

/// Parse volume CSV output
pub(crate) fn parse_wmic_volumes(csv: &str) -> Vec<VolumeInfo> {
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
        if drive_letter.is_empty() {
            continue;
        }

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

/// Get partitions for a disk
fn get_disk_partitions(_device_id: &str, volumes: &[VolumeInfo]) -> Vec<Partition> {
    // Simplified - maps volumes to partitions
    // Full implementation would use Win32_DiskDriveToDiskPartition
    volumes
        .iter()
        .map(|v| Partition {
            path: v.drive_letter.clone(),
            label: v.label.clone(),
            filesystem: v.filesystem.clone(),
            size: v.size,
            mount_point: Some(v.drive_letter.clone()),
        })
        .collect()
}

/// Detect drive type from interface and media type
pub(crate) fn detect_drive_type(interface_type: &str, media_type: &str) -> DriveType {
    match interface_type.to_uppercase().as_str() {
        "USB" => DriveType::Usb,
        "NVME" | "PCIE" => DriveType::Nvme,
        "SCSI" | "SATA" | "IDE" => {
            if media_type.contains("Removable") || media_type.contains("External") {
                DriveType::Usb
            } else {
                DriveType::Sata
            }
        }
        "SD" => DriveType::SdCard,
        _ => {
            if media_type.contains("Removable") {
                DriveType::Usb
            } else {
                DriveType::Other
            }
        }
    }
}

/// Check if this is a system drive
pub(crate) fn check_if_system_drive(
    media_type: &str,
    mount_points: &[String],
    interface_type: &str,
) -> (bool, Option<String>) {
    // Check for system drive letter
    for mp in mount_points {
        if is_system_mount_point(mp) {
            return (
                true,
                Some(format!("Contains Windows system drive: {}", mp)),
            );
        }
    }

    // Fixed internal drives are likely system drives
    if media_type == "Fixed hard disk media" && interface_type.to_uppercase() != "USB" {
        return (
            true,
            Some("Fixed internal hard disk".to_string()),
        );
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
    // parse_wmic_disks tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_parse_wmic_disks_basic() {
        let csv = r#"
Node,DeviceID,Index,InterfaceType,MediaType,Model,SerialNumber,Size
DESKTOP,\\.\PHYSICALDRIVE0,0,SCSI,Fixed hard disk media,Samsung SSD 970,S123456,512110190592
DESKTOP,\\.\PHYSICALDRIVE1,1,USB,Removable Media,SanDisk Ultra,,32015679488
"#;
        let disks = parse_wmic_disks(csv).unwrap();
        assert_eq!(disks.len(), 2);
        
        assert_eq!(disks[0].index, 0);
        assert_eq!(disks[0].model, "Samsung SSD 970");
        assert_eq!(disks[0].size, 512110190592);
        assert_eq!(disks[0].interface_type, "SCSI");
        assert_eq!(disks[0].serial, Some("S123456".to_string()));
        
        assert_eq!(disks[1].index, 1);
        assert_eq!(disks[1].model, "SanDisk Ultra");
        assert_eq!(disks[1].interface_type, "USB");
        assert!(disks[1].serial.is_none());
    }

    #[test]
    fn test_parse_wmic_disks_empty() {
        let csv = "";
        let disks = parse_wmic_disks(csv).unwrap();
        assert!(disks.is_empty());
    }

    #[test]
    fn test_parse_wmic_disks_headers_only() {
        let csv = "Node,DeviceID,Index,InterfaceType,MediaType,Model,SerialNumber,Size";
        let disks = parse_wmic_disks(csv).unwrap();
        assert!(disks.is_empty());
    }

    #[test]
    fn test_parse_wmic_disks_skip_zero_size() {
        let csv = r#"
Node,DeviceID,Index,InterfaceType,MediaType,Model,SerialNumber,Size
DESKTOP,\\.\PHYSICALDRIVE0,0,SCSI,Fixed hard disk media,Drive1,,0
DESKTOP,\\.\PHYSICALDRIVE1,1,USB,Removable Media,Drive2,,32015679488
"#;
        let disks = parse_wmic_disks(csv).unwrap();
        assert_eq!(disks.len(), 1);
        assert_eq!(disks[0].model, "Drive2");
    }

    // -------------------------------------------------------------------------
    // parse_wmic_volumes tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_parse_wmic_volumes_basic() {
        let csv = r#"
Node,Capacity,DriveLetter,FileSystem,Label
DESKTOP,511999156224,C:,NTFS,Windows
DESKTOP,32010928128,D:,FAT32,USB_DRIVE
"#;
        let volumes = parse_wmic_volumes(csv);
        assert_eq!(volumes.len(), 2);
        
        assert_eq!(volumes[0].drive_letter, "C:");
        assert_eq!(volumes[0].filesystem, Some("NTFS".to_string()));
        assert_eq!(volumes[0].label, Some("Windows".to_string()));
        
        assert_eq!(volumes[1].drive_letter, "D:");
        assert_eq!(volumes[1].filesystem, Some("FAT32".to_string()));
    }

    #[test]
    fn test_parse_wmic_volumes_skip_empty_drive_letter() {
        let csv = r#"
Node,Capacity,DriveLetter,FileSystem,Label
DESKTOP,511999156224,,NTFS,Recovery
DESKTOP,32010928128,E:,FAT32,USB
"#;
        let volumes = parse_wmic_volumes(csv);
        assert_eq!(volumes.len(), 1);
        assert_eq!(volumes[0].drive_letter, "E:");
    }

    // -------------------------------------------------------------------------
    // detect_drive_type tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_detect_drive_type_usb() {
        assert_eq!(detect_drive_type("USB", "Removable Media"), DriveType::Usb);
        assert_eq!(detect_drive_type("usb", "Removable Media"), DriveType::Usb);
    }

    #[test]
    fn test_detect_drive_type_nvme() {
        assert_eq!(detect_drive_type("NVMe", "Fixed hard disk media"), DriveType::Nvme);
        assert_eq!(detect_drive_type("PCIE", "Fixed hard disk media"), DriveType::Nvme);
    }

    #[test]
    fn test_detect_drive_type_sata() {
        assert_eq!(detect_drive_type("SATA", "Fixed hard disk media"), DriveType::Sata);
        assert_eq!(detect_drive_type("SCSI", "Fixed hard disk media"), DriveType::Sata);
        assert_eq!(detect_drive_type("IDE", "Fixed hard disk media"), DriveType::Sata);
    }

    #[test]
    fn test_detect_drive_type_external_sata() {
        // External SATA drives should be detected as USB (removable)
        assert_eq!(detect_drive_type("SATA", "External hard disk media"), DriveType::Usb);
        assert_eq!(detect_drive_type("SCSI", "Removable Media"), DriveType::Usb);
    }

    #[test]
    fn test_detect_drive_type_sd() {
        assert_eq!(detect_drive_type("SD", "Removable Media"), DriveType::SdCard);
    }

    #[test]
    fn test_detect_drive_type_unknown_removable() {
        assert_eq!(detect_drive_type("Unknown", "Removable Media"), DriveType::Usb);
    }

    #[test]
    fn test_detect_drive_type_unknown_fixed() {
        assert_eq!(detect_drive_type("Unknown", "Fixed hard disk media"), DriveType::Other);
    }

    // -------------------------------------------------------------------------
    // check_if_system_drive tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_check_if_system_drive_c_drive() {
        let (is_system, reason) = check_if_system_drive(
            "Fixed hard disk media",
            &["C:".to_string()],
            "SCSI",
        );
        assert!(is_system);
        assert!(reason.unwrap().contains("C:"));
    }

    #[test]
    fn test_check_if_system_drive_c_windows() {
        let (is_system, _) = check_if_system_drive(
            "Fixed hard disk media",
            &["C:\\Windows".to_string()],
            "SCSI",
        );
        assert!(is_system);
    }

    #[test]
    fn test_check_if_system_drive_fixed_internal() {
        let (is_system, reason) = check_if_system_drive(
            "Fixed hard disk media",
            &["D:".to_string()],
            "SCSI",
        );
        assert!(is_system);
        assert!(reason.unwrap().contains("Fixed internal"));
    }

    #[test]
    fn test_check_if_system_drive_usb() {
        let (is_system, _) = check_if_system_drive(
            "Removable Media",
            &["E:".to_string()],
            "USB",
        );
        assert!(!is_system);
    }

    #[test]
    fn test_check_if_system_drive_external_usb_fixed() {
        // USB drive with "Fixed hard disk media" (like some external HDDs)
        let (is_system, _) = check_if_system_drive(
            "Fixed hard disk media",
            &["F:".to_string()],
            "USB",
        );
        assert!(!is_system); // USB interface overrides the fixed media type
    }

    // -------------------------------------------------------------------------
    // Integration tests (require actual Windows system)
    // -------------------------------------------------------------------------

    #[test]
    #[ignore]
    fn test_list_drives_real() {
        let drives = list_drives();
        assert!(drives.is_ok(), "Should be able to list drives");
        
        let drives = drives.unwrap();
        assert!(!drives.is_empty(), "Should find at least one drive");
        
        // C: drive should be present and marked as system
        let has_system = drives.iter().any(|d| d.is_system);
        assert!(has_system, "Should identify a system drive");
    }
}
