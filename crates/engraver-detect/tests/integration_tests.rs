//! Integration tests for engraver-detect
//!
//! These tests verify the public API works correctly.

use engraver_detect::*;

// ============================================================================
// Drive struct tests
// ============================================================================

#[test]
fn test_drive_builder_pattern() {
    let drive = Drive::new("/dev/sdb")
        .with_name("Test USB Drive")
        .with_size(32 * 1024 * 1024 * 1024)
        .with_removable(true)
        .with_drive_type(DriveType::Usb)
        .with_mount_point("/media/usb");

    assert_eq!(drive.path, "/dev/sdb");
    assert_eq!(drive.name, "Test USB Drive");
    assert!(drive.removable);
    assert!(!drive.is_system);
    assert!(drive.is_safe_target());
    assert_eq!(drive.mount_points, vec!["/media/usb".to_string()]);
}

#[test]
fn test_drive_safety_combinations() {
    // Test all combinations of removable/system flags

    // removable=true, is_system=false -> SAFE
    let drive = Drive::new("/dev/sdb")
        .with_removable(true)
        .with_system(false, None);
    assert!(
        drive.is_safe_target(),
        "Removable non-system drive should be safe"
    );

    // removable=true, is_system=true -> UNSAFE
    let drive = Drive::new("/dev/sdb")
        .with_removable(true)
        .with_system(true, Some("Test".to_string()));
    assert!(
        !drive.is_safe_target(),
        "Removable system drive should not be safe"
    );

    // removable=false, is_system=false -> UNSAFE
    let drive = Drive::new("/dev/sda")
        .with_removable(false)
        .with_system(false, None);
    assert!(
        !drive.is_safe_target(),
        "Non-removable drive should not be safe"
    );

    // removable=false, is_system=true -> UNSAFE
    let drive = Drive::new("/dev/sda")
        .with_removable(false)
        .with_system(true, Some("Root".to_string()));
    assert!(
        !drive.is_safe_target(),
        "Non-removable system drive should not be safe"
    );
}

// ============================================================================
// Size formatting tests
// ============================================================================

#[test]
fn test_format_bytes_edge_cases() {
    // Edge cases
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(1), "1 B");
    #[allow(clippy::cast_precision_loss)]
    let expected = format!(
        "{:.1} TB",
        u64::MAX as f64 / (1024.0 * 1024.0 * 1024.0 * 1024.0)
    );
    assert_eq!(format_bytes(u64::MAX), expected);
}

#[test]
fn test_format_bytes_boundaries() {
    // Exactly at boundaries
    assert_eq!(format_bytes(1023), "1023 B");
    assert_eq!(format_bytes(1024), "1.0 KB");
    assert_eq!(format_bytes(1024 * 1024 - 1), "1024.0 KB");
    assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
}

// ============================================================================
// System mount point tests
// ============================================================================

#[test]
fn test_system_mount_point_detection() {
    // Should be detected as system
    let system_mounts = [
        "/",
        "/boot",
        "/home",
        "/usr",
        "/var",
        "/System",
        "/Applications",
        "C:\\",
        "C:\\Windows",
        "c:\\windows",
    ];

    for mount in system_mounts {
        assert!(
            is_system_mount_point(mount),
            "{mount} should be detected as system mount"
        );
    }
}

#[test]
fn test_non_system_mount_points() {
    // Should NOT be detected as system
    let safe_mounts = [
        "/mnt/usb",
        "/media/user/USB",
        "/run/media/user/DISK",
        "/Volumes/USB_DRIVE",
        "D:\\",
        "E:\\Data",
        "/tmp",
        "/custom/mount",
    ];

    for mount in safe_mounts {
        assert!(
            !is_system_mount_point(mount),
            "{mount} should NOT be detected as system mount"
        );
    }
}

// ============================================================================
// DriveType tests
// ============================================================================

#[test]
fn test_drive_type_display() {
    assert_eq!(format!("{}", DriveType::Usb), "USB");
    assert_eq!(format!("{}", DriveType::SdCard), "SD Card");
    assert_eq!(format!("{}", DriveType::Nvme), "NVMe");
    assert_eq!(format!("{}", DriveType::Sata), "SATA");
    assert_eq!(format!("{}", DriveType::Other), "Other");
}

#[test]
fn test_drive_type_serialization() {
    let drive_type = DriveType::Usb;
    let json = serde_json::to_string(&drive_type).unwrap();
    assert_eq!(json, "\"Usb\"");

    let deserialized: DriveType = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized, DriveType::Usb);
}

// ============================================================================
// Partition tests
// ============================================================================

#[test]
fn test_partition_creation() {
    let partition = Partition {
        path: "/dev/sdb1".to_string(),
        label: Some("UBUNTU".to_string()),
        filesystem: Some("vfat".to_string()),
        size: 32 * 1024 * 1024 * 1024,
        mount_point: Some("/media/usb".to_string()),
    };

    assert_eq!(partition.path, "/dev/sdb1");
    assert_eq!(partition.label, Some("UBUNTU".to_string()));
    assert_eq!(partition.filesystem, Some("vfat".to_string()));
}

// ============================================================================
// Error tests
// ============================================================================

#[test]
fn test_error_types() {
    let err = DetectError::EnumerationFailed("test".to_string());
    assert!(err.to_string().contains("test"));

    let err = DetectError::PermissionDenied("need sudo".to_string());
    assert!(err.to_string().contains("Permission denied"));

    let err = DetectError::UnsupportedPlatform;
    assert!(err.to_string().contains("not supported"));
}

// ============================================================================
// API tests (platform-dependent)
// ============================================================================

#[test]
#[ignore = "requires actual drives, run with: cargo test -- --ignored"]
fn test_list_drives_returns_result() {
    let result = list_drives();
    assert!(result.is_ok() || matches!(result, Err(DetectError::UnsupportedPlatform)));
}

#[test]
#[ignore = "requires actual drives, run with: cargo test -- --ignored"]
fn test_list_removable_drives_filters() {
    if let Ok(all) = list_all_drives() {
        if let Ok(removable) = list_removable_drives() {
            // Removable should be a subset of all
            assert!(removable.len() <= all.len());

            // All removable drives should be in the all list
            for r in &removable {
                assert!(all.iter().any(|a| a.path == r.path));
            }

            // All removable drives should be safe targets
            for r in &removable {
                assert!(r.is_safe_target());
            }
        }
    }
}

#[test]
#[ignore = "requires actual drives, run with: cargo test -- --ignored"]
fn test_validate_target_rejects_system_drives() {
    if let Ok(drives) = list_all_drives() {
        for drive in drives {
            if drive.is_system {
                let result = validate_target(&drive.path);
                assert!(
                    result.is_err(),
                    "Should reject system drive: {}",
                    drive.path
                );
            }
        }
    }
}
