//! Windows drive detection implementation
//!
//! Uses PowerShell and WMI/CIM queries for device enumeration.

use super::{is_system_mount_point, DetectError, Drive, DriveType, Partition, Result, UsbSpeed};
use std::collections::HashMap;
use std::process::Command;

/// List all drives on Windows
pub fn list_drives() -> Result<Vec<Drive>> {
    let disks = get_physical_disks()?;
    let volumes = get_volumes()?;
    let volume_disks = get_volume_disks();

    let mut drives = Vec::new();

    for disk in &disks {
        let DiskVolumes {
            partitions,
            mount_points,
            is_system,
            system_reason,
        } = disk_volumes(disk, &disks, &volumes, &volume_disks);

        let raw_path = format!("\\\\.\\PhysicalDrive{}", disk.index);

        let removable =
            disk.media_type.contains("Removable") || disk.media_type.contains("External");

        let drive_type = detect_drive_type(&disk.interface_type, &disk.media_type);

        // Get USB speed for USB devices
        let usb_speed = if drive_type == DriveType::Usb {
            get_usb_speed_for_disk(disk)
        } else {
            None
        };

        drives.push(Drive {
            path: raw_path.clone(),
            raw_path,
            name: disk.model.clone(),
            size: disk.size,
            removable,
            is_system,
            drive_type,
            vendor: None,
            model: Some(disk.model.clone()),
            serial: disk.serial.clone(),
            mount_points,
            partitions,
            system_reason,
            usb_speed,
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

/// Get physical disks using PowerShell Get-CimInstance
fn get_physical_disks() -> Result<Vec<PhysicalDisk>> {
    let ps_command = r#"Get-CimInstance -ClassName Win32_DiskDrive | Select-Object Index,DeviceID,Model,Size,MediaType,InterfaceType,SerialNumber | ConvertTo-Csv -NoTypeInformation"#;

    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", ps_command])
        .output()
        .map_err(|e| DetectError::CommandFailed {
            message: format!("PowerShell failed"),
            source: Some(e),
        })?;

    if !output.status.success() {
        return Err(DetectError::CommandFailed {
            message: format!(
                "PowerShell failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ),
            source: None,
        });
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    parse_powershell_disks(&output_str)
}

/// Parse PowerShell CSV output for disks
pub(crate) fn parse_powershell_disks(csv: &str) -> Result<Vec<PhysicalDisk>> {
    let mut disks = Vec::new();
    let mut lines = csv.lines().peekable();

    // First line should be headers
    let headers: Vec<String> = match lines.next() {
        Some(line) => parse_csv_line(line),
        None => return Ok(disks),
    };

    if headers.is_empty() {
        return Ok(disks);
    }

    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let fields = parse_csv_line(line);
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
        let model = row.get("Model").unwrap_or(&"Unknown").to_string();
        let size = row
            .get("Size")
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);
        let media_type = row.get("MediaType").unwrap_or(&"").to_string();
        let interface_type = row.get("InterfaceType").unwrap_or(&"").to_string();
        let serial = row
            .get("SerialNumber")
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());

        // Skip disks with zero size
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

/// Parse a CSV line handling quoted fields
fn parse_csv_line(line: &str) -> Vec<String> {
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

/// Volume info
#[derive(Debug, Clone)]
pub(crate) struct VolumeInfo {
    pub drive_letter: String,
    pub label: Option<String>,
    pub filesystem: Option<String>,
    pub size: u64,
}

/// Get volumes using PowerShell Get-CimInstance
fn get_volumes() -> Result<Vec<VolumeInfo>> {
    let ps_command = r#"Get-CimInstance -ClassName Win32_Volume | Where-Object { $_.DriveLetter -ne $null } | Select-Object DriveLetter,Label,FileSystem,Capacity | ConvertTo-Csv -NoTypeInformation"#;

    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", ps_command])
        .output()
        .map_err(|e| DetectError::CommandFailed {
            message: format!("PowerShell failed"),
            source: Some(e),
        })?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    Ok(parse_powershell_volumes(&output_str))
}

/// Parse PowerShell CSV output for volumes
pub(crate) fn parse_powershell_volumes(csv: &str) -> Vec<VolumeInfo> {
    let mut volumes = Vec::new();
    let mut lines = csv.lines().peekable();

    // First line should be headers
    let headers: Vec<String> = match lines.next() {
        Some(line) => parse_csv_line(line),
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

        let fields = parse_csv_line(line);
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

/// The physical disks that hold each drive letter, keyed like `VolumeInfo.drive_letter`
/// (`"C:"`). Values are `Win32_DiskDrive.Index`, which equals `Get-Partition`'s `DiskNumber`.
/// A letter can list several disks when a volume spans them.
pub(crate) type VolumeDisks = HashMap<String, Vec<u32>>;

/// Map drive letters to the disks that hold them using PowerShell Get-Partition
///
/// Fails closed. On any failure the map is empty, and `get_disk_partitions` gives every
/// volume to every disk, so every disk is treated as holding the system drive.
fn get_volume_disks() -> VolumeDisks {
    let ps_command = r#"Get-Partition | Where-Object DriveLetter | Select-Object DiskNumber,DriveLetter | ConvertTo-Csv -NoTypeInformation"#;

    let output = match Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", ps_command])
        .output()
    {
        Ok(output) if output.status.success() => output,
        Ok(output) => {
            tracing::debug!(
                "Get-Partition failed, assigning every volume to every disk: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return VolumeDisks::new();
        }
        Err(e) => {
            tracing::debug!(
                "Could not run Get-Partition, assigning every volume to every disk: {e}"
            );
            return VolumeDisks::new();
        }
    };

    let volume_disks = parse_partition_disks(&String::from_utf8_lossy(&output.stdout));
    if volume_disks.is_empty() {
        tracing::debug!(
            "Get-Partition listed no drive letters, assigning every volume to every disk"
        );
    }
    volume_disks
}

/// Parse PowerShell CSV output for the drive letter to disk map
pub(crate) fn parse_partition_disks(csv: &str) -> VolumeDisks {
    let mut volume_disks = VolumeDisks::new();
    let mut lines = csv.lines();

    let headers: Vec<String> = match lines.next() {
        Some(line) => parse_csv_line(line),
        None => return volume_disks,
    };
    let column = |name: &str| headers.iter().position(|h| h == name);
    let (Some(disk_column), Some(letter_column)) = (column("DiskNumber"), column("DriveLetter"))
    else {
        return volume_disks;
    };

    for line in lines {
        let fields = parse_csv_line(line.trim());
        if fields.len() != headers.len() {
            continue;
        }

        let Some(letter) = normalize_drive_letter(&fields[letter_column]) else {
            continue;
        };
        let Ok(disk) = fields[disk_column].parse::<u32>() else {
            continue;
        };

        let disks = volume_disks.entry(letter).or_default();
        if !disks.contains(&disk) {
            disks.push(disk);
        }
    }

    volume_disks
}

/// Turn a `Get-Partition` drive letter (`C`) into the `C:` form `VolumeInfo` uses
///
/// Rows for partitions without a letter hold an empty string or a NUL char, which give `None`.
fn normalize_drive_letter(raw: &str) -> Option<String> {
    let mut chars = raw.trim().trim_end_matches(':').chars();
    match (chars.next(), chars.next()) {
        (Some(letter), None) if letter.is_ascii_alphabetic() => {
            Some(format!("{}:", letter.to_ascii_uppercase()))
        }
        _ => None,
    }
}

/// What `list_drives` reports for one disk from the volumes it holds.
struct DiskVolumes {
    partitions: Vec<Partition>,
    mount_points: Vec<String>,
    is_system: bool,
    system_reason: Option<String>,
}

/// The volumes of `disk`, and the system verdict they give. `listed` is every enumerated disk.
fn disk_volumes(
    disk: &PhysicalDisk,
    listed: &[PhysicalDisk],
    volumes: &[VolumeInfo],
    volume_disks: &VolumeDisks,
) -> DiskVolumes {
    let partitions = get_disk_partitions(disk.index, listed, volumes, volume_disks);
    let mount_points: Vec<String> = partitions
        .iter()
        .filter_map(|p| p.mount_point.clone())
        .collect();
    let (is_system, system_reason) =
        check_if_system_drive(&disk.media_type, &mount_points, &disk.interface_type);

    DiskVolumes {
        partitions,
        mount_points,
        is_system,
        system_reason,
    }
}

/// Get partitions for a disk
///
/// Fails closed. A letter goes only to the disks the map names, unless the map names none of
/// the listed disks, and then it goes to every disk. A `C:` the map lacks, or places on a disk
/// that is not listed (the host disk of a native-boot VHDX), can never make the real system
/// disk look safe.
fn get_disk_partitions(
    disk_index: u32,
    listed: &[PhysicalDisk],
    volumes: &[VolumeInfo],
    volume_disks: &VolumeDisks,
) -> Vec<Partition> {
    volumes
        .iter()
        .filter(|v| {
            volume_disks
                .get(&v.drive_letter.to_ascii_uppercase())
                .filter(|named| listed.iter().any(|d| named.contains(&d.index)))
                .is_none_or(|named| named.contains(&disk_index))
        })
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

/// Get USB speed for a device
///
/// Uses PowerShell to query USB controller information via CIM/WMI.
/// Matches the device by PNP Device ID or serial number.
fn get_usb_speed_for_disk(disk: &PhysicalDisk) -> Option<UsbSpeed> {
    // Only check USB devices
    if disk.interface_type.to_uppercase() != "USB" {
        return None;
    }

    // Try to get USB speed via PowerShell CIM query
    // This queries Win32_USBHub and related classes
    let ps_command = r#"
        Get-CimInstance -ClassName Win32_USBHub |
        Select-Object DeviceID, Status, USBVersion |
        ConvertTo-Csv -NoTypeInformation
    "#;

    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", ps_command])
        .output()
        .ok()?;

    if !output.status.success() {
        // Fall back to checking if the device ID contains USB version hints
        return detect_usb_speed_from_device_id(&disk.device_id);
    }

    let output_str = String::from_utf8_lossy(&output.stdout);
    parse_usb_speed_from_powershell(&output_str, disk)
}

/// Parse USB speed from PowerShell output
pub(crate) fn parse_usb_speed_from_powershell(csv: &str, disk: &PhysicalDisk) -> Option<UsbSpeed> {
    // The output includes USB version info
    // Try to match by looking at USBVersion field

    for line in csv.lines().skip(1) {
        // Skip header
        let fields: Vec<&str> = line.split(',').map(|s| s.trim_matches('"')).collect();

        if fields.len() >= 3 {
            let usb_version = fields.get(2).unwrap_or(&"");

            // Check if this hub is related to our device
            // This is a heuristic - we check if the device ID matches
            if let Some(device_id) = fields.first() {
                // USB version strings are like "2.0", "3.0", "3.1", etc.
                if let Some(speed) = parse_usb_version_string(usb_version) {
                    // If we can't match specific device, return the best speed we find
                    // for USB devices (this is a simplification)
                    if disk.interface_type.to_uppercase() == "USB" {
                        // Try to find a matching controller
                        if device_id.contains(&disk.index.to_string()) {
                            return Some(speed);
                        }
                    }
                }
            }
        }
    }

    // Fall back to device ID detection
    detect_usb_speed_from_device_id(&disk.device_id)
}

/// Try to detect USB speed from the device ID string
///
/// Device IDs sometimes contain hints like "USB3" or "USBSTOR"
fn detect_usb_speed_from_device_id(device_id: &str) -> Option<UsbSpeed> {
    let upper = device_id.to_uppercase();

    if upper.contains("USB3") || upper.contains("XHCI") {
        // USB 3.x device - assume SuperSpeed unless we know better
        Some(UsbSpeed::SuperSpeed)
    } else if upper.contains("USB2") || upper.contains("EHCI") {
        Some(UsbSpeed::High)
    } else if upper.contains("USB") || upper.contains("USBSTOR") {
        // Generic USB - assume USB 2.0 as conservative estimate
        Some(UsbSpeed::High)
    } else {
        None
    }
}

/// Parse a USB version string like "2.0", "3.0", "3.1", "3.2"
fn parse_usb_version_string(version: &str) -> Option<UsbSpeed> {
    let version = version.trim();

    if version.starts_with("3.2") || version.starts_with("4") {
        Some(UsbSpeed::SuperSpeedPlus20)
    } else if version.starts_with("3.1") {
        Some(UsbSpeed::SuperSpeedPlus)
    } else if version.starts_with("3.0") || version.starts_with("3") {
        Some(UsbSpeed::SuperSpeed)
    } else if version.starts_with("2.0") || version.starts_with("2") {
        Some(UsbSpeed::High)
    } else if version.starts_with("1.1") {
        Some(UsbSpeed::Full)
    } else if version.starts_with("1.0") || version.starts_with("1") {
        Some(UsbSpeed::Low)
    } else {
        None
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
            return (true, Some(format!("Contains Windows system drive: {}", mp)));
        }
    }

    // Fixed internal drives are likely system drives
    if media_type == "Fixed hard disk media" && interface_type.to_uppercase() != "USB" {
        return (true, Some("Fixed internal hard disk".to_string()));
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
    // parse_powershell_disks tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_parse_powershell_disks_basic() {
        // PowerShell CSV format: quoted fields, no Node column
        let csv = r#""Index","DeviceID","Model","Size","MediaType","InterfaceType","SerialNumber"
"0","\\.\PHYSICALDRIVE0","Samsung SSD 970","512110190592","Fixed hard disk media","SCSI","S123456"
"1","\\.\PHYSICALDRIVE1","SanDisk Ultra","32015679488","Removable Media","USB",""
"#;
        let disks = parse_powershell_disks(csv).unwrap();
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
    fn test_parse_powershell_disks_empty() {
        let csv = "";
        let disks = parse_powershell_disks(csv).unwrap();
        assert!(disks.is_empty());
    }

    #[test]
    fn test_parse_powershell_disks_headers_only() {
        let csv = r#""Index","DeviceID","Model","Size","MediaType","InterfaceType","SerialNumber""#;
        let disks = parse_powershell_disks(csv).unwrap();
        assert!(disks.is_empty());
    }

    #[test]
    fn test_parse_powershell_disks_skip_zero_size() {
        let csv = r#""Index","DeviceID","Model","Size","MediaType","InterfaceType","SerialNumber"
"0","\\.\PHYSICALDRIVE0","Drive1","0","Fixed hard disk media","SCSI",""
"1","\\.\PHYSICALDRIVE1","Drive2","32015679488","Removable Media","USB",""
"#;
        let disks = parse_powershell_disks(csv).unwrap();
        assert_eq!(disks.len(), 1);
        assert_eq!(disks[0].model, "Drive2");
    }

    // -------------------------------------------------------------------------
    // parse_powershell_volumes tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_parse_powershell_volumes_basic() {
        // PowerShell CSV format: quoted fields
        let csv = r#""DriveLetter","Label","FileSystem","Capacity"
"C:","Windows","NTFS","511999156224"
"D:","USB_DRIVE","FAT32","32010928128"
"#;
        let volumes = parse_powershell_volumes(csv);
        assert_eq!(volumes.len(), 2);

        assert_eq!(volumes[0].drive_letter, "C:");
        assert_eq!(volumes[0].filesystem, Some("NTFS".to_string()));
        assert_eq!(volumes[0].label, Some("Windows".to_string()));

        assert_eq!(volumes[1].drive_letter, "D:");
        assert_eq!(volumes[1].filesystem, Some("FAT32".to_string()));
    }

    #[test]
    fn test_parse_powershell_volumes_skip_empty_drive_letter() {
        let csv = r#""DriveLetter","Label","FileSystem","Capacity"
"","Recovery","NTFS","511999156224"
"E:","USB","FAT32","32010928128"
"#;
        let volumes = parse_powershell_volumes(csv);
        assert_eq!(volumes.len(), 1);
        assert_eq!(volumes[0].drive_letter, "E:");
    }

    // -------------------------------------------------------------------------
    // Volume to disk mapping tests
    // -------------------------------------------------------------------------

    /// Disk 0 is the internal system disk, disk 1 is a removable USB stick.
    const TWO_DISKS_CSV: &str = r#""Index","DeviceID","Model","Size","MediaType","InterfaceType","SerialNumber"
"0","\\.\PHYSICALDRIVE0","Samsung SSD 970","512110190592","Fixed hard disk media","SCSI","S123456"
"1","\\.\PHYSICALDRIVE1","SanDisk Ultra","32015679488","Removable Media","USB",""
"#;

    /// `C:` lives on disk 0 and `E:` on disk 1.
    const C_AND_E_VOLUMES_CSV: &str = r#""DriveLetter","Label","FileSystem","Capacity"
"C:","Windows","NTFS","511999156224"
"E:","USB","FAT32","32010928128"
"#;

    /// `Get-Partition` output for the layout above.
    const C_ON_0_E_ON_1_CSV: &str = r#""DiskNumber","DriveLetter"
"0","C"
"1","E"
"#;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn test_each_disk_gets_only_its_own_volumes() {
        let disks = parse_powershell_disks(TWO_DISKS_CSV).unwrap();
        let volumes = parse_powershell_volumes(C_AND_E_VOLUMES_CSV);
        let volume_disks = parse_partition_disks(C_ON_0_E_ON_1_CSV);

        let held = disk_volumes(&disks[0], &disks, &volumes, &volume_disks);
        assert_eq!(held.mount_points, strings(&["C:"]));
        assert!(
            held.is_system,
            "the disk holding C: must be the system disk"
        );

        let held = disk_volumes(&disks[1], &disks, &volumes, &volume_disks);
        assert_eq!(held.mount_points, strings(&["E:"]));
        assert!(!held.is_system, "the USB disk must not be the system disk");
    }

    #[test]
    fn test_empty_map_gives_every_disk_every_volume() {
        // Get-Partition failed or listed nothing: fail closed.
        let disks = parse_powershell_disks(TWO_DISKS_CSV).unwrap();
        let volumes = parse_powershell_volumes(C_AND_E_VOLUMES_CSV);
        let volume_disks = parse_partition_disks("");

        for disk in &disks {
            let held = disk_volumes(disk, &disks, &volumes, &volume_disks);
            assert_eq!(held.mount_points, strings(&["C:", "E:"]));
            assert!(
                held.is_system,
                "{} must be treated as a system disk",
                disk.model
            );
        }
    }

    #[test]
    fn test_unmapped_volume_lands_on_every_disk() {
        let disks = parse_powershell_disks(TWO_DISKS_CSV).unwrap();
        let volumes = parse_powershell_volumes(
            r#""DriveLetter","Label","FileSystem","Capacity"
"C:","Windows","NTFS","511999156224"
"E:","USB","FAT32","32010928128"
"F:","Data","NTFS","1000000000"
"#,
        );
        let volume_disks = parse_partition_disks(C_ON_0_E_ON_1_CSV);

        let held = disk_volumes(&disks[0], &disks, &volumes, &volume_disks);
        assert_eq!(held.mount_points, strings(&["C:", "F:"]));
        let held = disk_volumes(&disks[1], &disks, &volumes, &volume_disks);
        assert_eq!(held.mount_points, strings(&["E:", "F:"]));
    }

    #[test]
    fn test_unmapped_system_volume_makes_every_disk_system() {
        // Get-Partition omitted C: (Storage Spaces, dynamic disks). The real system disk
        // must not look safe, so C: goes to every disk.
        let disks = parse_powershell_disks(TWO_DISKS_CSV).unwrap();
        let volumes = parse_powershell_volumes(C_AND_E_VOLUMES_CSV);
        let volume_disks = parse_partition_disks("\"DiskNumber\",\"DriveLetter\"\n\"1\",\"E\"\n");

        let held = disk_volumes(&disks[0], &disks, &volumes, &volume_disks);
        assert_eq!(held.mount_points, strings(&["C:"]));
        assert!(held.is_system);

        let held = disk_volumes(&disks[1], &disks, &volumes, &volume_disks);
        assert_eq!(held.mount_points, strings(&["C:", "E:"]));
        assert!(held.is_system);
    }

    #[test]
    fn test_volume_on_an_unlisted_disk_lands_on_every_disk() {
        // C: sits on disk 5, which Win32_DiskDrive does not list, as with a native-boot VHDX.
        // The real host disk must not look safe, so C: goes to every listed disk.
        let disks = parse_powershell_disks(TWO_DISKS_CSV).unwrap();
        let volumes = parse_powershell_volumes(
            r#""DriveLetter","Label","FileSystem","Capacity"
"C:","Windows","NTFS","511999156224"
"#,
        );
        let volume_disks = parse_partition_disks("\"DiskNumber\",\"DriveLetter\"\n\"5\",\"C\"\n");

        for disk in &disks {
            let held = disk_volumes(disk, &disks, &volumes, &volume_disks);
            assert_eq!(held.mount_points, strings(&["C:"]), "{}", disk.model);
            assert!(
                held.is_system,
                "{} must be treated as a system disk",
                disk.model
            );
        }
    }

    #[test]
    fn test_volume_spanning_disks_is_listed_on_each_of_them() {
        let disks = parse_powershell_disks(TWO_DISKS_CSV).unwrap();
        let volumes = parse_powershell_volumes(C_AND_E_VOLUMES_CSV);
        let volume_disks = parse_partition_disks(
            "\"DiskNumber\",\"DriveLetter\"\n\"0\",\"C\"\n\"1\",\"C\"\n\"1\",\"E\"\n",
        );

        let held = disk_volumes(&disks[0], &disks, &volumes, &volume_disks);
        assert_eq!(held.mount_points, strings(&["C:"]));
        assert!(held.is_system);

        let held = disk_volumes(&disks[1], &disks, &volumes, &volume_disks);
        assert_eq!(held.mount_points, strings(&["C:", "E:"]));
        assert!(held.is_system);
    }

    // -------------------------------------------------------------------------
    // parse_partition_disks tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_parse_partition_disks_basic() {
        // Get-Partition reports the letter without a colon
        let csv = r#""DiskNumber","DriveLetter"
"0","C"
"1","E"
"1","F"
"#;
        let map = parse_partition_disks(csv);
        assert_eq!(map.len(), 3);
        assert_eq!(map["C:"], vec![0]);
        assert_eq!(map["E:"], vec![1]);
        assert_eq!(map["F:"], vec![1]);
    }

    #[test]
    fn test_parse_partition_disks_skips_partitions_without_a_letter() {
        // An empty DriveLetter, and the NUL char PowerShell prints for "no letter"
        let csv = "\"DiskNumber\",\"DriveLetter\"\n\"0\",\"\"\n\"0\",\"\0\"\n\"1\",\"E\"\n";
        let map = parse_partition_disks(csv);
        assert_eq!(map.len(), 1);
        assert_eq!(map["E:"], vec![1]);
    }

    #[test]
    fn test_parse_partition_disks_header_only() {
        assert!(parse_partition_disks(r#""DiskNumber","DriveLetter""#).is_empty());
        assert!(parse_partition_disks("").is_empty());
    }

    #[test]
    fn test_parse_partition_disks_finds_columns_by_name() {
        let csv = "\"DriveLetter\",\"DiskNumber\"\n\"C\",\"0\"\n";
        assert_eq!(parse_partition_disks(csv)["C:"], vec![0]);

        // Without the expected columns nothing can be mapped
        assert!(parse_partition_disks("\"Foo\",\"Bar\"\n\"C\",\"0\"\n").is_empty());
    }

    #[test]
    fn test_parse_partition_disks_normalizes_letters() {
        let csv = "\"DiskNumber\",\"DriveLetter\"\n\"0\",\"c\"\n\"1\",\"E:\"\n";
        let map = parse_partition_disks(csv);
        assert_eq!(map["C:"], vec![0]);
        assert_eq!(map["E:"], vec![1]);
    }

    #[test]
    fn test_parse_partition_disks_skips_unusable_rows() {
        let csv = "\"DiskNumber\",\"DriveLetter\"\n\"x\",\"C\"\n\"0\",\"CD\"\n\"0\"\n\"1\",\"E\"\n";
        let map = parse_partition_disks(csv);
        assert_eq!(map.len(), 1);
        assert_eq!(map["E:"], vec![1]);
    }

    #[test]
    fn test_parse_partition_disks_collects_every_disk_of_a_letter_once() {
        let csv = "\"DiskNumber\",\"DriveLetter\"\n\"0\",\"D\"\n\"1\",\"D\"\n\"1\",\"D\"\n";
        assert_eq!(parse_partition_disks(csv)["D:"], vec![0, 1]);
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
        assert_eq!(
            detect_drive_type("NVMe", "Fixed hard disk media"),
            DriveType::Nvme
        );
        assert_eq!(
            detect_drive_type("PCIE", "Fixed hard disk media"),
            DriveType::Nvme
        );
    }

    #[test]
    fn test_detect_drive_type_sata() {
        assert_eq!(
            detect_drive_type("SATA", "Fixed hard disk media"),
            DriveType::Sata
        );
        assert_eq!(
            detect_drive_type("SCSI", "Fixed hard disk media"),
            DriveType::Sata
        );
        assert_eq!(
            detect_drive_type("IDE", "Fixed hard disk media"),
            DriveType::Sata
        );
    }

    #[test]
    fn test_detect_drive_type_external_sata() {
        // External SATA drives should be detected as USB (removable)
        assert_eq!(
            detect_drive_type("SATA", "External hard disk media"),
            DriveType::Usb
        );
        assert_eq!(detect_drive_type("SCSI", "Removable Media"), DriveType::Usb);
    }

    #[test]
    fn test_detect_drive_type_sd() {
        assert_eq!(
            detect_drive_type("SD", "Removable Media"),
            DriveType::SdCard
        );
    }

    #[test]
    fn test_detect_drive_type_unknown_removable() {
        assert_eq!(
            detect_drive_type("Unknown", "Removable Media"),
            DriveType::Usb
        );
    }

    #[test]
    fn test_detect_drive_type_unknown_fixed() {
        assert_eq!(
            detect_drive_type("Unknown", "Fixed hard disk media"),
            DriveType::Other
        );
    }

    // -------------------------------------------------------------------------
    // check_if_system_drive tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_check_if_system_drive_c_drive() {
        let (is_system, reason) =
            check_if_system_drive("Fixed hard disk media", &["C:".to_string()], "SCSI");
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
        let (is_system, reason) =
            check_if_system_drive("Fixed hard disk media", &["D:".to_string()], "SCSI");
        assert!(is_system);
        assert!(reason.unwrap().contains("Fixed internal"));
    }

    #[test]
    fn test_check_if_system_drive_usb() {
        let (is_system, _) = check_if_system_drive("Removable Media", &["E:".to_string()], "USB");
        assert!(!is_system);
    }

    #[test]
    fn test_check_if_system_drive_external_usb_fixed() {
        // USB drive with "Fixed hard disk media" (like some external HDDs)
        let (is_system, _) =
            check_if_system_drive("Fixed hard disk media", &["F:".to_string()], "USB");
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

    #[cfg(windows)]
    #[test]
    fn test_list_drives_real_maps_each_volume_to_its_own_disk() {
        let drives = list_drives().expect("Should be able to list drives");
        let holds = |drive: &Drive, letter: &str| {
            drive
                .mount_points
                .iter()
                .any(|m| m.eq_ignore_ascii_case(letter))
        };

        // The disk holding C: is the system disk.
        let system_holders: Vec<&Drive> = drives.iter().filter(|d| holds(d, "C:")).collect();
        assert!(!system_holders.is_empty(), "No disk holds C:");
        for drive in system_holders {
            assert!(
                drive.is_system,
                "{} holds C: but is not is_system",
                drive.path
            );
        }

        // A plain install lists C:. Without it, the query or the parser broke and
        // the checks below would pass with every disk falling back to system.
        let volume_disks = get_volume_disks();
        assert!(
            volume_disks.contains_key("C:"),
            "Get-Partition gave no disk for C:: {volume_disks:?}"
        );

        // A letter Get-Partition did not list, or put only on disks that are not listed, goes to
        // every disk by design, so only the other letters are checked.
        let is_listed = |index: &u32| {
            drives
                .iter()
                .any(|d| d.path == format!("\\\\.\\PhysicalDrive{index}"))
        };
        for (letter, disks) in volume_disks.iter().filter(|(_, d)| d.iter().any(is_listed)) {
            let holders = drives.iter().filter(|d| holds(d, letter)).count();
            assert!(
                holders <= disks.len(),
                "{letter} is on {holders} disks, but Get-Partition puts it on {disks:?}"
            );
        }
    }

    // -------------------------------------------------------------------------
    // USB speed detection tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_parse_usb_version_string() {
        assert_eq!(
            parse_usb_version_string("3.2"),
            Some(UsbSpeed::SuperSpeedPlus20)
        );
        assert_eq!(
            parse_usb_version_string("3.1"),
            Some(UsbSpeed::SuperSpeedPlus)
        );
        assert_eq!(parse_usb_version_string("3.0"), Some(UsbSpeed::SuperSpeed));
        assert_eq!(parse_usb_version_string("2.0"), Some(UsbSpeed::High));
        assert_eq!(parse_usb_version_string("1.1"), Some(UsbSpeed::Full));
        assert_eq!(parse_usb_version_string("1.0"), Some(UsbSpeed::Low));
        assert_eq!(parse_usb_version_string(""), None);
        assert_eq!(parse_usb_version_string("unknown"), None);
    }

    #[test]
    fn test_detect_usb_speed_from_device_id() {
        assert_eq!(
            detect_usb_speed_from_device_id("USB\\VID_0781&PID_5591\\ABC123"),
            Some(UsbSpeed::High)
        );
        assert_eq!(
            detect_usb_speed_from_device_id("USB3\\VID_0781&PID_5591\\ABC123"),
            Some(UsbSpeed::SuperSpeed)
        );
        assert_eq!(
            detect_usb_speed_from_device_id("USBSTOR\\DISK&VEN_SANDISK"),
            Some(UsbSpeed::High)
        );
        assert_eq!(
            detect_usb_speed_from_device_id("SCSI\\DISK&VEN_SAMSUNG"),
            None
        );
    }
}
