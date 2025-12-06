//! # Engraver Detect
//!
//! Safe drive detection and enumeration with system drive protection.
//! This is a safety-critical component that prevents accidental overwrites.
//!
//! ## Safety Philosophy
//!
//! This crate uses multiple heuristics to identify system drives:
//! - Drives containing mount points like `/`, `/home`, `C:\`
//! - Non-removable internal drives
//! - Drives with system partitions (EFI, Recovery, etc.)
//!
//! When in doubt, we err on the side of caution and mark drives as unsafe.

#![warn(missing_docs)]
#![warn(clippy::all)]

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

/// Drive detection errors
#[derive(Error, Debug)]
pub enum DetectError {
    /// Failed to enumerate drives
    #[error("Failed to enumerate drives: {0}")]
    EnumerationFailed(String),

    /// Permission denied when accessing drive information
    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    /// Platform not supported
    #[error("Platform not supported")]
    UnsupportedPlatform,

    /// Command execution failed
    #[error("Command failed: {0}")]
    CommandFailed(String),

    /// Failed to parse drive information
    #[error("Parse error: {0}")]
    ParseError(String),

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result type for drive detection operations
pub type Result<T> = std::result::Result<T, DetectError>;

/// Type of drive connection
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DriveType {
    /// USB connected drive
    Usb,
    /// SD card (via built-in or USB reader)
    SdCard,
    /// NVMe drive (external/portable)
    Nvme,
    /// SATA drive
    Sata,
    /// Other/unknown connection type
    Other,
}

impl Default for DriveType {
    fn default() -> Self {
        DriveType::Other
    }
}

impl fmt::Display for DriveType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriveType::Usb => write!(f, "USB"),
            DriveType::SdCard => write!(f, "SD Card"),
            DriveType::Nvme => write!(f, "NVMe"),
            DriveType::Sata => write!(f, "SATA"),
            DriveType::Other => write!(f, "Other"),
        }
    }
}

/// Represents a detected drive/device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Drive {
    /// Device path (e.g., /dev/sdb, /dev/disk2, \\.\PhysicalDrive1)
    pub path: String,

    /// Raw device path for direct I/O (may differ from path on some platforms)
    pub raw_path: String,

    /// Human-readable name/model
    pub name: String,

    /// Size in bytes
    pub size: u64,

    /// Whether this is a removable drive
    pub removable: bool,

    /// Whether this appears to be a system drive
    pub is_system: bool,

    /// Type of drive connection
    pub drive_type: DriveType,

    /// Vendor name if available
    pub vendor: Option<String>,

    /// Model name if available
    pub model: Option<String>,

    /// Serial number if available
    pub serial: Option<String>,

    /// Mount points if mounted
    pub mount_points: Vec<String>,

    /// Partition information
    pub partitions: Vec<Partition>,

    /// Why this drive was marked as system (if applicable)
    pub system_reason: Option<String>,
}

impl Default for Drive {
    fn default() -> Self {
        Self {
            path: String::new(),
            raw_path: String::new(),
            name: String::new(),
            size: 0,
            removable: false,
            is_system: false,
            drive_type: DriveType::Other,
            vendor: None,
            model: None,
            serial: None,
            mount_points: Vec::new(),
            partitions: Vec::new(),
            system_reason: None,
        }
    }
}

/// Represents a partition on a drive
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Partition {
    /// Partition path (e.g., /dev/sdb1)
    pub path: String,

    /// Partition label if available
    pub label: Option<String>,

    /// Filesystem type if known
    pub filesystem: Option<String>,

    /// Size in bytes
    pub size: u64,

    /// Mount point if mounted
    pub mount_point: Option<String>,
}

impl Drive {
    /// Create a new Drive with the given path
    pub fn new(path: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            raw_path: path.clone(),
            path,
            ..Default::default()
        }
    }

    /// Builder: set the name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// Builder: set the size
    pub fn with_size(mut self, size: u64) -> Self {
        self.size = size;
        self
    }

    /// Builder: set removable flag
    pub fn with_removable(mut self, removable: bool) -> Self {
        self.removable = removable;
        self
    }

    /// Builder: set system flag
    pub fn with_system(mut self, is_system: bool, reason: Option<String>) -> Self {
        self.is_system = is_system;
        self.system_reason = reason;
        self
    }

    /// Builder: set drive type
    pub fn with_drive_type(mut self, drive_type: DriveType) -> Self {
        self.drive_type = drive_type;
        self
    }

    /// Builder: add mount point
    pub fn with_mount_point(mut self, mount_point: impl Into<String>) -> Self {
        self.mount_points.push(mount_point.into());
        self
    }

    /// Check if this drive is safe to write to
    ///
    /// Returns false for:
    /// - System drives
    /// - Non-removable drives (unless explicitly allowed)
    /// - Drives with active system mount points
    pub fn is_safe_target(&self) -> bool {
        self.removable && !self.is_system
    }

    /// Format size for human-readable display
    pub fn size_display(&self) -> String {
        format_bytes(self.size)
    }

    /// Get a display string for the drive
    pub fn display_name(&self) -> String {
        let vendor = self.vendor.as_deref().unwrap_or("");
        let model = self.model.as_deref().unwrap_or(&self.name);

        if vendor.is_empty() {
            model.to_string()
        } else {
            format!("{} {}", vendor, model).trim().to_string()
        }
    }
}

/// Format bytes into human-readable string
pub fn format_bytes(bytes: u64) -> String {
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

/// System mount points that indicate a system drive
pub const SYSTEM_MOUNT_POINTS: &[&str] = &[
    "/",
    "/boot",
    "/boot/efi",
    "/home",
    "/usr",
    "/var",
    "/etc",
    "/System",
    "/Applications",
    "/Library",
    "C:\\",
    "C:\\Windows",
];

/// Check if any mount point indicates a system drive
pub fn is_system_mount_point(mount_point: &str) -> bool {
    // Normalize the path
    let normalized = mount_point.trim();
    
    SYSTEM_MOUNT_POINTS
        .iter()
        .any(|&sys| {
            normalized == sys 
            || normalized.eq_ignore_ascii_case(sys)
            || normalized.starts_with(&format!("{}\\", sys))
            || normalized.starts_with(&format!("{}/", sys))
        })
}

// Platform-specific implementations
cfg_if::cfg_if! {
    if #[cfg(target_os = "linux")] {
        mod linux;
        pub use linux::list_drives;
    } else if #[cfg(target_os = "macos")] {
        mod macos;
        pub use macos::list_drives;
    } else if #[cfg(target_os = "windows")] {
        mod windows;
        pub use windows::list_drives;
    } else {
        /// List removable drives (unsupported platform)
        pub fn list_drives() -> Result<Vec<Drive>> {
            Err(DetectError::UnsupportedPlatform)
        }
    }
}

/// List all removable drives suitable for imaging
///
/// This is the main entry point for drive detection. It returns
/// only drives that are safe to write to by default.
pub fn list_removable_drives() -> Result<Vec<Drive>> {
    let drives = list_drives()?;
    Ok(drives.into_iter().filter(|d| d.is_safe_target()).collect())
}

/// List all drives including system drives
///
/// Use with caution - includes drives that should NOT be written to.
pub fn list_all_drives() -> Result<Vec<Drive>> {
    list_drives()
}

/// Validate that a device path is safe to write to
///
/// Returns the Drive if valid and safe, or an error explaining why not.
pub fn validate_target(device_path: &str) -> Result<Drive> {
    let drives = list_drives()?;

    // Find the drive
    let drive = drives
        .into_iter()
        .find(|d| d.path == device_path || d.raw_path == device_path)
        .ok_or_else(|| {
            DetectError::EnumerationFailed(format!("Device not found: {}", device_path))
        })?;

    // Check if safe
    if drive.is_system {
        return Err(DetectError::EnumerationFailed(format!(
            "Refusing to use system drive: {} ({})",
            device_path,
            drive.system_reason.as_deref().unwrap_or("system drive detected")
        )));
    }

    if !drive.removable {
        return Err(DetectError::EnumerationFailed(format!(
            "Drive is not removable: {}. Use --force to override (dangerous!)",
            device_path
        )));
    }

    Ok(drive)
}

// ============================================================================
// UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------------------------
    // format_bytes tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_format_bytes_zero() {
        assert_eq!(format_bytes(0), "0 B");
    }

    #[test]
    fn test_format_bytes_bytes() {
        assert_eq!(format_bytes(1), "1 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1023), "1023 B");
    }

    #[test]
    fn test_format_bytes_kilobytes() {
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(10 * 1024), "10.0 KB");
    }

    #[test]
    fn test_format_bytes_megabytes() {
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(500 * 1024 * 1024), "500.0 MB");
    }

    #[test]
    fn test_format_bytes_gigabytes() {
        assert_eq!(format_bytes(1024 * 1024 * 1024), "1.0 GB");
        assert_eq!(format_bytes(32 * 1024 * 1024 * 1024), "32.0 GB");
        assert_eq!(format_bytes(64 * 1024 * 1024 * 1024), "64.0 GB");
        assert_eq!(format_bytes(128 * 1024 * 1024 * 1024), "128.0 GB");
    }

    #[test]
    fn test_format_bytes_terabytes() {
        assert_eq!(format_bytes(1024 * 1024 * 1024 * 1024), "1.0 TB");
        assert_eq!(format_bytes(2 * 1024 * 1024 * 1024 * 1024), "2.0 TB");
    }

    #[test]
    fn test_format_bytes_common_sizes() {
        // Common USB drive sizes
        assert_eq!(format_bytes(8_000_000_000), "7.5 GB");      // "8GB" USB
        assert_eq!(format_bytes(16_000_000_000), "14.9 GB");    // "16GB" USB
        assert_eq!(format_bytes(32_000_000_000), "29.8 GB");    // "32GB" USB
        assert_eq!(format_bytes(64_000_000_000), "59.6 GB");    // "64GB" USB
        assert_eq!(format_bytes(128_000_000_000), "119.2 GB");  // "128GB" USB
    }

    // -------------------------------------------------------------------------
    // is_system_mount_point tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_system_mount_points_linux() {
        assert!(is_system_mount_point("/"));
        assert!(is_system_mount_point("/boot"));
        assert!(is_system_mount_point("/boot/efi"));
        assert!(is_system_mount_point("/home"));
        assert!(is_system_mount_point("/usr"));
        assert!(is_system_mount_point("/var"));
        assert!(is_system_mount_point("/etc"));
    }

    #[test]
    fn test_system_mount_points_macos() {
        assert!(is_system_mount_point("/System"));
        assert!(is_system_mount_point("/Applications"));
        assert!(is_system_mount_point("/Library"));
    }

    #[test]
    fn test_system_mount_points_windows() {
        assert!(is_system_mount_point("C:\\"));
        assert!(is_system_mount_point("C:\\Windows"));
        assert!(is_system_mount_point("c:\\"));  // Case insensitive
        assert!(is_system_mount_point("c:\\windows"));
    }

    #[test]
    fn test_non_system_mount_points() {
        assert!(!is_system_mount_point("/mnt/usb"));
        assert!(!is_system_mount_point("/media/user/USB_DRIVE"));
        assert!(!is_system_mount_point("/Volumes/USB"));
        assert!(!is_system_mount_point("D:\\"));
        assert!(!is_system_mount_point("E:\\"));
        assert!(!is_system_mount_point("/run/media/user/disk"));
    }

    #[test]
    fn test_system_mount_points_edge_cases() {
        assert!(!is_system_mount_point(""));
        assert!(!is_system_mount_point("   "));
        assert!(is_system_mount_point("  /  "));  // Trimmed
        assert!(!is_system_mount_point("/not_system"));
        assert!(!is_system_mount_point("/home_backup"));
    }

    // -------------------------------------------------------------------------
    // Drive tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_drive_default() {
        let drive = Drive::default();
        assert!(drive.path.is_empty());
        assert_eq!(drive.size, 0);
        assert!(!drive.removable);
        assert!(!drive.is_system);
        assert_eq!(drive.drive_type, DriveType::Other);
    }

    #[test]
    fn test_drive_builder() {
        let drive = Drive::new("/dev/sdb")
            .with_name("Test Drive")
            .with_size(32 * 1024 * 1024 * 1024)
            .with_removable(true)
            .with_drive_type(DriveType::Usb);

        assert_eq!(drive.path, "/dev/sdb");
        assert_eq!(drive.name, "Test Drive");
        assert_eq!(drive.size, 32 * 1024 * 1024 * 1024);
        assert!(drive.removable);
        assert_eq!(drive.drive_type, DriveType::Usb);
    }

    #[test]
    fn test_drive_is_safe_target() {
        // Safe: removable and not system
        let safe_drive = Drive::new("/dev/sdb")
            .with_removable(true)
            .with_system(false, None);
        assert!(safe_drive.is_safe_target());

        // Unsafe: system drive
        let system_drive = Drive::new("/dev/sda")
            .with_removable(true)
            .with_system(true, Some("Root filesystem".to_string()));
        assert!(!system_drive.is_safe_target());

        // Unsafe: not removable
        let internal_drive = Drive::new("/dev/sda")
            .with_removable(false)
            .with_system(false, None);
        assert!(!internal_drive.is_safe_target());

        // Unsafe: both non-removable and system
        let dangerous_drive = Drive::new("/dev/sda")
            .with_removable(false)
            .with_system(true, Some("System drive".to_string()));
        assert!(!dangerous_drive.is_safe_target());
    }

    #[test]
    fn test_drive_display_name() {
        // Vendor and model
        let drive = Drive {
            vendor: Some("SanDisk".to_string()),
            model: Some("Ultra Fit".to_string()),
            name: "Fallback".to_string(),
            ..Default::default()
        };
        assert_eq!(drive.display_name(), "SanDisk Ultra Fit");

        // Only model
        let drive = Drive {
            vendor: None,
            model: Some("Kingston DataTraveler".to_string()),
            name: "Fallback".to_string(),
            ..Default::default()
        };
        assert_eq!(drive.display_name(), "Kingston DataTraveler");

        // Only vendor (uses name as model fallback)
        let drive = Drive {
            vendor: Some("Generic".to_string()),
            model: None,
            name: "USB Drive".to_string(),
            ..Default::default()
        };
        assert_eq!(drive.display_name(), "Generic USB Drive");

        // Neither vendor nor model
        let drive = Drive {
            vendor: None,
            model: None,
            name: "USB Drive".to_string(),
            ..Default::default()
        };
        assert_eq!(drive.display_name(), "USB Drive");
    }

    #[test]
    fn test_drive_size_display() {
        let drive = Drive::new("/dev/sdb").with_size(32 * 1024 * 1024 * 1024);
        assert_eq!(drive.size_display(), "32.0 GB");

        let drive = Drive::new("/dev/sdc").with_size(1024);
        assert_eq!(drive.size_display(), "1.0 KB");
    }

    #[test]
    fn test_drive_serialization() {
        let drive = Drive::new("/dev/sdb")
            .with_name("Test")
            .with_size(1024)
            .with_removable(true)
            .with_drive_type(DriveType::Usb);

        let json = serde_json::to_string(&drive).expect("Should serialize");
        assert!(json.contains("/dev/sdb"));
        assert!(json.contains("Usb"));

        let deserialized: Drive = serde_json::from_str(&json).expect("Should deserialize");
        assert_eq!(deserialized.path, "/dev/sdb");
        assert_eq!(deserialized.drive_type, DriveType::Usb);
    }

    // -------------------------------------------------------------------------
    // DriveType tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_drive_type_display() {
        assert_eq!(DriveType::Usb.to_string(), "USB");
        assert_eq!(DriveType::SdCard.to_string(), "SD Card");
        assert_eq!(DriveType::Nvme.to_string(), "NVMe");
        assert_eq!(DriveType::Sata.to_string(), "SATA");
        assert_eq!(DriveType::Other.to_string(), "Other");
    }

    #[test]
    fn test_drive_type_default() {
        assert_eq!(DriveType::default(), DriveType::Other);
    }

    #[test]
    fn test_drive_type_equality() {
        assert_eq!(DriveType::Usb, DriveType::Usb);
        assert_ne!(DriveType::Usb, DriveType::Sata);
    }

    // -------------------------------------------------------------------------
    // Partition tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_partition_default() {
        let part = Partition::default();
        assert!(part.path.is_empty());
        assert!(part.label.is_none());
        assert!(part.filesystem.is_none());
        assert_eq!(part.size, 0);
        assert!(part.mount_point.is_none());
    }

    #[test]
    fn test_partition_serialization() {
        let part = Partition {
            path: "/dev/sdb1".to_string(),
            label: Some("UBUNTU".to_string()),
            filesystem: Some("vfat".to_string()),
            size: 512 * 1024 * 1024,
            mount_point: Some("/media/usb".to_string()),
        };

        let json = serde_json::to_string(&part).expect("Should serialize");
        let deserialized: Partition = serde_json::from_str(&json).expect("Should deserialize");
        
        assert_eq!(deserialized.path, "/dev/sdb1");
        assert_eq!(deserialized.label, Some("UBUNTU".to_string()));
    }

    // -------------------------------------------------------------------------
    // Error tests
    // -------------------------------------------------------------------------

    #[test]
    fn test_error_display() {
        let err = DetectError::EnumerationFailed("test error".to_string());
        assert_eq!(err.to_string(), "Failed to enumerate drives: test error");

        let err = DetectError::PermissionDenied("need root".to_string());
        assert_eq!(err.to_string(), "Permission denied: need root");

        let err = DetectError::UnsupportedPlatform;
        assert_eq!(err.to_string(), "Platform not supported");
    }
}
