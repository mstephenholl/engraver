//! # Engraver Detect
//!
//! Safe drive detection and enumeration with system drive protection.
//! This is a safety-critical component that prevents accidental overwrites.

#![warn(missing_docs)]
#![warn(clippy::all)]

use serde::{Deserialize, Serialize};
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
}

/// Result type for drive detection operations
pub type Result<T> = std::result::Result<T, DetectError>;

/// Represents a detected drive/device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Drive {
    /// Device path (e.g., /dev/sdb, /dev/disk2)
    pub path: String,

    /// Human-readable name/model
    pub name: String,

    /// Size in bytes
    pub size: u64,

    /// Whether this is a removable drive
    pub removable: bool,

    /// Whether this appears to be a system drive
    pub is_system: bool,

    /// Vendor name if available
    pub vendor: Option<String>,

    /// Serial number if available
    pub serial: Option<String>,

    /// Mount points if mounted
    pub mount_points: Vec<String>,
}

impl Drive {
    /// Check if this drive is safe to write to
    ///
    /// Returns false for system drives and non-removable drives
    pub fn is_safe_target(&self) -> bool {
        self.removable && !self.is_system
    }

    /// Format size for human-readable display
    pub fn size_display(&self) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;
        const TB: u64 = GB * 1024;

        if self.size >= TB {
            format!("{:.1} TB", self.size as f64 / TB as f64)
        } else if self.size >= GB {
            format!("{:.1} GB", self.size as f64 / GB as f64)
        } else if self.size >= MB {
            format!("{:.1} MB", self.size as f64 / MB as f64)
        } else if self.size >= KB {
            format!("{:.1} KB", self.size as f64 / KB as f64)
        } else {
            format!("{} B", self.size)
        }
    }
}

/// List all removable drives suitable for imaging
pub fn list_removable_drives() -> Result<Vec<Drive>> {
    cfg_if::cfg_if! {
        if #[cfg(target_os = "linux")] {
            linux::list_drives()
        } else if #[cfg(target_os = "macos")] {
            macos::list_drives()
        } else if #[cfg(target_os = "windows")] {
            windows::list_drives()
        } else {
            Err(DetectError::UnsupportedPlatform)
        }
    }
}

/// Validate that a device path is safe to write to
pub fn validate_target(device_path: &str) -> Result<Drive> {
    let drives = list_removable_drives()?;
    
    drives
        .into_iter()
        .find(|d| d.path == device_path && d.is_safe_target())
        .ok_or_else(|| DetectError::EnumerationFailed(
            format!("Device {} is not a valid/safe target", device_path)
        ))
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;

    pub fn list_drives() -> Result<Vec<Drive>> {
        // TODO: Implement using /sys/block, udev, or lsblk
        tracing::debug!("Listing drives on Linux");
        Ok(Vec::new())
    }
}

#[cfg(target_os = "macos")]
mod macos {
    use super::*;

    pub fn list_drives() -> Result<Vec<Drive>> {
        // TODO: Implement using diskutil or IOKit
        tracing::debug!("Listing drives on macOS");
        Ok(Vec::new())
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::*;

    pub fn list_drives() -> Result<Vec<Drive>> {
        // TODO: Implement using WMI or SetupAPI
        tracing::debug!("Listing drives on Windows");
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_size_display() {
        let drive = Drive {
            path: "/dev/sdb".to_string(),
            name: "Test Drive".to_string(),
            size: 32 * 1024 * 1024 * 1024, // 32 GB
            removable: true,
            is_system: false,
            vendor: None,
            serial: None,
            mount_points: vec![],
        };

        assert_eq!(drive.size_display(), "32.0 GB");
        assert!(drive.is_safe_target());
    }

    #[test]
    fn test_system_drive_not_safe() {
        let drive = Drive {
            path: "/dev/sda".to_string(),
            name: "System Drive".to_string(),
            size: 500 * 1024 * 1024 * 1024,
            removable: false,
            is_system: true,
            vendor: None,
            serial: None,
            mount_points: vec!["/".to_string()],
        };

        assert!(!drive.is_safe_target());
    }
}
