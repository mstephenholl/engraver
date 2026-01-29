//! Partition table inspection for source images
//!
//! This module provides functionality to read and display partition table
//! information from disk images before writing them.
//!
//! Supports both MBR (Master Boot Record) and GPT (GUID Partition Table) formats.

use crate::error::{Error, Result};
use crate::source::Source;
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom};

#[cfg(feature = "partition-info")]
use bootsector::{list_partitions, Options};

/// Minimum bytes needed for partition table inspection
/// GPT requires LBA 0-33 (34 sectors * 512 = 17,408 bytes)
/// We use 64KB to be safe for 4K sector disks
pub const PARTITION_HEADER_SIZE: usize = 64 * 1024;

/// Type of partition table found in the image
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PartitionTableType {
    /// GUID Partition Table (modern)
    Gpt,
    /// Master Boot Record (legacy)
    Mbr,
    /// No recognizable partition table
    None,
}

impl std::fmt::Display for PartitionTableType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PartitionTableType::Gpt => write!(f, "GPT"),
            PartitionTableType::Mbr => write!(f, "MBR"),
            PartitionTableType::None => write!(f, "None"),
        }
    }
}

/// Information about a single partition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionInfo {
    /// Partition number (1-indexed for user display)
    pub number: u32,
    /// Start offset in bytes
    pub start_offset: u64,
    /// Size in bytes
    pub size: u64,
    /// Partition type (human-readable name)
    pub partition_type: String,
    /// Partition type code/UUID (raw identifier)
    pub type_id: String,
    /// Partition name/label if available (GPT only)
    pub name: Option<String>,
    /// Whether this is a bootable/active partition
    pub bootable: bool,
}

/// Complete partition table information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PartitionTableInfo {
    /// Type of partition table
    pub table_type: PartitionTableType,
    /// Disk identifier (MBR signature or GPT GUID)
    pub disk_id: Option<String>,
    /// Sector size (typically 512 or 4096)
    pub sector_size: u32,
    /// List of partitions
    pub partitions: Vec<PartitionInfo>,
}

impl Default for PartitionTableInfo {
    fn default() -> Self {
        Self {
            table_type: PartitionTableType::None,
            disk_id: None,
            sector_size: 512,
            partitions: Vec::new(),
        }
    }
}

/// Read the partition header from a source
///
/// This reads enough data from the beginning of a source to parse partition tables.
/// Works with both seekable and non-seekable (compressed) sources.
pub fn read_partition_header(source_path: &str) -> Result<Vec<u8>> {
    let mut source = Source::open(source_path)?;
    let mut buffer = vec![0u8; PARTITION_HEADER_SIZE];
    let bytes_read = source.read(&mut buffer).map_err(|e| {
        Error::PartitionParseError(format!("Failed to read partition header: {}", e))
    })?;
    buffer.truncate(bytes_read);
    Ok(buffer)
}

/// Inspect partition table from a seekable source
#[cfg(feature = "partition-info")]
pub fn inspect_partitions<R: Read + Seek>(source: &mut R) -> Result<PartitionTableInfo> {
    // Seek to beginning
    source
        .seek(SeekFrom::Start(0))
        .map_err(|e| Error::PartitionParseError(format!("Failed to seek to beginning: {}", e)))?;

    // Read partition header
    let mut buffer = vec![0u8; PARTITION_HEADER_SIZE];
    let bytes_read = source
        .read(&mut buffer)
        .map_err(|e| Error::PartitionParseError(format!("Failed to read partition data: {}", e)))?;
    buffer.truncate(bytes_read);

    // Parse from buffer
    inspect_from_buffer(&buffer)
}

/// Inspect partition table from raw bytes
#[cfg(feature = "partition-info")]
pub fn inspect_from_buffer(buffer: &[u8]) -> Result<PartitionTableInfo> {
    if buffer.len() < 512 {
        return Ok(PartitionTableInfo::default());
    }

    // Use the byte slice directly as it implements ReadAt
    let options = Options::default();

    match list_partitions(buffer, &options) {
        Ok(partitions) => {
            let mut info = PartitionTableInfo::default();

            // Determine table type and extract partition info
            for (idx, partition) in partitions.iter().enumerate() {
                match &partition.attributes {
                    bootsector::Attributes::GPT {
                        type_uuid,
                        partition_uuid,
                        name,
                        attributes,
                    } => {
                        info.table_type = PartitionTableType::Gpt;

                        let partition_type = gpt_type_name(type_uuid);
                        // Legacy BIOS bootable flag is bit 2 of attributes (as u64)
                        let attr_flags = u64::from_le_bytes(*attributes);
                        let bootable = (attr_flags & 0x04) != 0;

                        info.partitions.push(PartitionInfo {
                            number: (idx + 1) as u32,
                            start_offset: partition.first_byte,
                            size: partition.len,
                            partition_type,
                            type_id: format_guid(type_uuid),
                            name: if name.is_empty() {
                                None
                            } else {
                                Some(name.clone())
                            },
                            bootable,
                        });

                        // Store partition UUID as disk ID if not set
                        if info.disk_id.is_none() && idx == 0 {
                            info.disk_id = Some(format_guid(partition_uuid));
                        }
                    }
                    bootsector::Attributes::MBR {
                        type_code,
                        bootable,
                    } => {
                        info.table_type = PartitionTableType::Mbr;
                        if info.disk_id.is_none() {
                            // Extract MBR signature from buffer bytes 440-443
                            if buffer.len() >= 444 {
                                let sig = u32::from_le_bytes([
                                    buffer[440],
                                    buffer[441],
                                    buffer[442],
                                    buffer[443],
                                ]);
                                info.disk_id = Some(format!("0x{:08X}", sig));
                            }
                        }

                        let partition_type = mbr_type_name(*type_code);

                        info.partitions.push(PartitionInfo {
                            number: (idx + 1) as u32,
                            start_offset: partition.first_byte,
                            size: partition.len,
                            partition_type,
                            type_id: format!("0x{:02X}", type_code),
                            name: None,
                            bootable: *bootable,
                        });
                    }
                }
            }

            Ok(info)
        }
        Err(_) => {
            // No valid partition table found
            Ok(PartitionTableInfo::default())
        }
    }
}

/// Stub implementation when partition-info feature is disabled
#[cfg(not(feature = "partition-info"))]
pub fn inspect_partitions<R: Read + Seek>(_source: &mut R) -> Result<PartitionTableInfo> {
    Err(Error::PartitionParseError(
        "Partition inspection not available (compiled without partition-info feature)".to_string(),
    ))
}

/// Stub implementation when partition-info feature is disabled
#[cfg(not(feature = "partition-info"))]
pub fn inspect_from_buffer(_buffer: &[u8]) -> Result<PartitionTableInfo> {
    Err(Error::PartitionParseError(
        "Partition inspection not available (compiled without partition-info feature)".to_string(),
    ))
}

/// Format a GUID as a string
#[cfg(feature = "partition-info")]
fn format_guid(guid: &[u8; 16]) -> String {
    format!(
        "{:02X}{:02X}{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        guid[3], guid[2], guid[1], guid[0],
        guid[5], guid[4],
        guid[7], guid[6],
        guid[8], guid[9],
        guid[10], guid[11], guid[12], guid[13], guid[14], guid[15]
    )
}

/// Get human-readable name for GPT partition type GUID
#[cfg(feature = "partition-info")]
fn gpt_type_name(guid: &[u8; 16]) -> String {
    // Convert to standard GUID format for comparison
    let guid_str = format_guid(guid).to_uppercase();

    match guid_str.as_str() {
        // EFI System Partition
        "C12A7328-F81F-11D2-BA4B-00A0C93EC93B" => "EFI System".to_string(),
        // Microsoft Basic Data (NTFS, FAT32)
        "EBD0A0A2-B9E5-4433-87C0-68B6B72699C7" => "Microsoft Basic Data".to_string(),
        // Microsoft Reserved
        "E3C9E316-0B5C-4DB8-817D-F92DF00215AE" => "Microsoft Reserved".to_string(),
        // Linux filesystem
        "0FC63DAF-8483-4772-8E79-3D69D8477DE4" => "Linux filesystem".to_string(),
        // Linux swap
        "0657FD6D-A4AB-43C4-84E5-0933C84B4F4F" => "Linux swap".to_string(),
        // Linux LVM
        "E6D6D379-F507-44C2-A23C-238F2A3DF928" => "Linux LVM".to_string(),
        // Linux RAID
        "A19D880F-05FC-4D3B-A006-743F0F84911E" => "Linux RAID".to_string(),
        // Linux home
        "933AC7E1-2EB4-4F13-B844-0E14E2AEF915" => "Linux /home".to_string(),
        // Linux root (x86-64)
        "4F68BCE3-E8CD-4DB1-96E7-FBCAF984B709" => "Linux root (x86-64)".to_string(),
        // BIOS boot partition
        "21686148-6449-6E6F-744E-656564454649" => "BIOS boot".to_string(),
        // Apple HFS+
        "48465300-0000-11AA-AA11-00306543ECAC" => "Apple HFS+".to_string(),
        // Apple APFS
        "7C3457EF-0000-11AA-AA11-00306543ECAC" => "Apple APFS".to_string(),
        // FreeBSD data
        "516E7CB4-6ECF-11D6-8FF8-00022D09712B" => "FreeBSD data".to_string(),
        _ => "Unknown".to_string(),
    }
}

/// Get human-readable name for MBR partition type code
#[cfg(feature = "partition-info")]
fn mbr_type_name(type_code: u8) -> String {
    match type_code {
        0x00 => "Empty".to_string(),
        0x01 => "FAT12".to_string(),
        0x04 => "FAT16 (<32M)".to_string(),
        0x05 => "Extended".to_string(),
        0x06 => "FAT16".to_string(),
        0x07 => "NTFS/HPFS".to_string(),
        0x0B => "W95 FAT32".to_string(),
        0x0C => "W95 FAT32 (LBA)".to_string(),
        0x0E => "W95 FAT16 (LBA)".to_string(),
        0x0F => "W95 Extended (LBA)".to_string(),
        0x11 => "Hidden FAT12".to_string(),
        0x14 => "Hidden FAT16 (<32M)".to_string(),
        0x16 => "Hidden FAT16".to_string(),
        0x17 => "Hidden NTFS".to_string(),
        0x1B => "Hidden W95 FAT32".to_string(),
        0x1C => "Hidden W95 FAT32 (LBA)".to_string(),
        0x1E => "Hidden W95 FAT16 (LBA)".to_string(),
        0x82 => "Linux swap".to_string(),
        0x83 => "Linux".to_string(),
        0x85 => "Linux extended".to_string(),
        0x8E => "Linux LVM".to_string(),
        0xA5 => "FreeBSD".to_string(),
        0xA6 => "OpenBSD".to_string(),
        0xA9 => "NetBSD".to_string(),
        0xAF => "Apple HFS+".to_string(),
        0xBE => "Solaris boot".to_string(),
        0xBF => "Solaris".to_string(),
        0xEE => "GPT protective".to_string(),
        0xEF => "EFI System".to_string(),
        0xFD => "Linux RAID".to_string(),
        _ => format!("Type 0x{:02X}", type_code),
    }
}

/// Format a size in bytes to human-readable form
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    const TB: u64 = GB * 1024;

    if bytes >= TB {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// Format partition offset for display (show in MB for readability)
pub fn format_offset(bytes: u64) -> String {
    const MB: u64 = 1024 * 1024;
    if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(format_size(1024 * 1024 * 1024 * 1024), "1.0 TB");
    }

    #[test]
    fn test_partition_table_type_display() {
        assert_eq!(format!("{}", PartitionTableType::Gpt), "GPT");
        assert_eq!(format!("{}", PartitionTableType::Mbr), "MBR");
        assert_eq!(format!("{}", PartitionTableType::None), "None");
    }

    #[test]
    fn test_default_partition_table_info() {
        let info = PartitionTableInfo::default();
        assert_eq!(info.table_type, PartitionTableType::None);
        assert!(info.partitions.is_empty());
        assert_eq!(info.sector_size, 512);
    }

    #[cfg(feature = "partition-info")]
    #[test]
    fn test_mbr_type_names() {
        assert_eq!(mbr_type_name(0x83), "Linux");
        assert_eq!(mbr_type_name(0x07), "NTFS/HPFS");
        assert_eq!(mbr_type_name(0xEF), "EFI System");
        assert_eq!(mbr_type_name(0xFF), "Type 0xFF");
    }

    #[cfg(feature = "partition-info")]
    #[test]
    fn test_inspect_empty_buffer() {
        let buffer = vec![0u8; 512];
        let result = inspect_from_buffer(&buffer);
        assert!(result.is_ok());
        let info = result.unwrap();
        assert_eq!(info.table_type, PartitionTableType::None);
    }

    #[cfg(feature = "partition-info")]
    #[test]
    fn test_inspect_small_buffer() {
        let buffer = vec![0u8; 100];
        let result = inspect_from_buffer(&buffer);
        assert!(result.is_ok());
        let info = result.unwrap();
        assert_eq!(info.table_type, PartitionTableType::None);
    }
}
