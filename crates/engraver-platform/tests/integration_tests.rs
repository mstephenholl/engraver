//! Integration tests for engraver-platform
//!
//! These tests verify the public API and cross-platform behavior.
//! Tests that require actual devices are marked with #[ignore].

use engraver_platform::*;
use std::io::Write;
use tempfile::NamedTempFile;

// ============================================================================
// Alignment utility tests
// ============================================================================

#[test]
fn test_align_up_powers_of_two() {
    // Test common block sizes
    for alignment in [512, 1024, 2048, 4096, 8192] {
        assert_eq!(align_up(0, alignment), 0);
        assert_eq!(align_up(1, alignment), alignment);
        assert_eq!(align_up(alignment - 1, alignment), alignment);
        assert_eq!(align_up(alignment, alignment), alignment);
        assert_eq!(align_up(alignment + 1, alignment), alignment * 2);
    }
}

#[test]
fn test_align_down_powers_of_two() {
    for alignment in [512, 1024, 2048, 4096, 8192] {
        assert_eq!(align_down(0, alignment), 0);
        assert_eq!(align_down(1, alignment), 0);
        assert_eq!(align_down(alignment - 1, alignment), 0);
        assert_eq!(align_down(alignment, alignment), alignment);
        assert_eq!(align_down(alignment + 1, alignment), alignment);
        assert_eq!(align_down(alignment * 2 - 1, alignment), alignment);
    }
}

#[test]
fn test_is_aligned_various_values() {
    assert!(is_aligned(0, 512));
    assert!(is_aligned(512, 512));
    assert!(is_aligned(1024, 512));
    assert!(is_aligned(4096, 512));
    assert!(is_aligned(4096, 4096));

    assert!(!is_aligned(1, 512));
    assert!(!is_aligned(511, 512));
    assert!(!is_aligned(513, 512));
    assert!(!is_aligned(4095, 4096));
}

// ============================================================================
// OpenOptions tests
// ============================================================================

#[test]
fn test_open_options_builder_chain() {
    let opts = OpenOptions::new()
        .direct_io(false)
        .read(true)
        .write(true)
        .block_size(512)
        .direct_io(true); // Can override

    assert!(opts.direct_io);
    assert!(opts.read);
    assert!(opts.write);
    assert_eq!(opts.block_size, 512);
}

#[test]
fn test_open_options_read_only() {
    let opts = OpenOptions::new().read(true).write(false);

    assert!(opts.read);
    assert!(!opts.write);
}

// ============================================================================
// DeviceInfo tests
// ============================================================================

#[test]
fn test_device_info_creation() {
    let info = DeviceInfo {
        path: "/dev/sdb".to_string(),
        size: 32 * 1024 * 1024 * 1024,
        block_size: 512,
        direct_io: true,
    };

    assert_eq!(info.path, "/dev/sdb");
    assert_eq!(info.size, 32 * 1024 * 1024 * 1024);
    assert_eq!(info.block_size, 512);
    assert!(info.direct_io);
}

// ============================================================================
// Error handling tests
// ============================================================================

#[test]
fn test_error_conversion_from_io() {
    let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "test");
    let platform_err: PlatformError = io_err.into();

    assert!(matches!(platform_err, PlatformError::Io(_)));
    assert!(platform_err.to_string().contains("IO error"));
}

#[test]
fn test_error_messages() {
    let errors = vec![
        PlatformError::PermissionDenied("test".to_string()),
        PlatformError::DeviceBusy("test".to_string()),
        PlatformError::DeviceNotFound("test".to_string()),
        PlatformError::UnmountFailed("test".to_string()),
        PlatformError::NotSupported("test".to_string()),
        PlatformError::CommandFailed("test".to_string()),
        PlatformError::AlignmentError("test".to_string()),
    ];

    for err in errors {
        let msg = err.to_string();
        assert!(!msg.is_empty());
        assert!(msg.contains("test") || msg.len() > 5);
    }
}

// ============================================================================
// File-based tests (work on all platforms)
// ============================================================================

#[test]
fn test_open_tempfile_without_direct_io() {
    let mut temp = NamedTempFile::new().unwrap();
    temp.write_all(&[0u8; 8192]).unwrap();

    let options = OpenOptions::new().direct_io(false);
    let result = open_device(temp.path().to_str().unwrap(), options);

    // Should work on regular files without direct I/O
    assert!(result.is_ok(), "Should open tempfile: {:?}", result.err());
}

#[test]
fn test_read_write_tempfile() {
    let mut temp = NamedTempFile::new().unwrap();
    temp.write_all(&[0u8; 16384]).unwrap();

    let options = OpenOptions::new().direct_io(false);
    let mut device = open_device(temp.path().to_str().unwrap(), options).unwrap();

    // Write test pattern
    let pattern: Vec<u8> = (0..256).map(|i| i as u8).collect();
    let written = device.write_at(0, &pattern).unwrap();
    assert_eq!(written, 256);

    // Sync
    device.sync().unwrap();

    // Read back
    let mut buffer = vec![0u8; 256];
    let read = device.read_at(0, &mut buffer).unwrap();
    assert_eq!(read, 256);
    assert_eq!(buffer, pattern);
}

#[test]
fn test_write_at_various_offsets() {
    let mut temp = NamedTempFile::new().unwrap();
    temp.write_all(&[0u8; 16384]).unwrap();

    let options = OpenOptions::new().direct_io(false);
    let mut device = open_device(temp.path().to_str().unwrap(), options).unwrap();

    // Write at different offsets
    let offsets = [0, 512, 1024, 4096, 8192];
    for (i, &offset) in offsets.iter().enumerate() {
        let data = vec![(i + 1) as u8; 64];
        device.write_at(offset, &data).unwrap();
    }

    // Verify each write
    for (i, &offset) in offsets.iter().enumerate() {
        let mut buffer = vec![0u8; 64];
        device.read_at(offset, &mut buffer).unwrap();
        assert!(
            buffer.iter().all(|&b| b == (i + 1) as u8),
            "Data mismatch at offset {}",
            offset
        );
    }
}

#[test]
fn test_device_size() {
    let mut temp = NamedTempFile::new().unwrap();
    let file_size = 32768u64;
    temp.write_all(&vec![0u8; file_size as usize]).unwrap();

    let options = OpenOptions::new().direct_io(false);
    let device = open_device(temp.path().to_str().unwrap(), options).unwrap();

    let info = device.info();
    assert!(
        info.size >= file_size,
        "Size should be at least {}",
        file_size
    );
}

#[test]
fn test_device_path_in_info() {
    let mut temp = NamedTempFile::new().unwrap();
    temp.write_all(&[0u8; 4096]).unwrap();

    let options = OpenOptions::new().direct_io(false);
    let device = open_device(temp.path().to_str().unwrap(), options).unwrap();

    let info = device.info();
    // Path should contain the temp file path (may be transformed on some platforms)
    assert!(!info.path.is_empty());
}

#[test]
fn test_sequential_writes() {
    let mut temp = NamedTempFile::new().unwrap();
    temp.write_all(&[0u8; 32768]).unwrap();

    let options = OpenOptions::new().direct_io(false);
    let mut device = open_device(temp.path().to_str().unwrap(), options).unwrap();

    // Write sequentially
    let chunk_size = 1024;
    for i in 0..8 {
        let data = vec![i as u8; chunk_size];
        let offset = i as u64 * chunk_size as u64;
        device.write_at(offset, &data).unwrap();
    }

    device.sync().unwrap();

    // Verify
    for i in 0..8 {
        let mut buffer = vec![0u8; chunk_size];
        let offset = i as u64 * chunk_size as u64;
        device.read_at(offset, &mut buffer).unwrap();
        assert!(
            buffer.iter().all(|&b| b == i as u8),
            "Mismatch at chunk {}",
            i
        );
    }
}

// ============================================================================
// Privilege check tests
// ============================================================================

#[test]
fn test_has_elevated_privileges_runs() {
    // Just verify it doesn't panic
    let _ = has_elevated_privileges();
}

// ============================================================================
// Error path tests
// ============================================================================

#[test]
fn test_open_nonexistent_file() {
    let result = open_device("/nonexistent/path/to/device", OpenOptions::default());
    assert!(result.is_err());

    if let Err(e) = result {
        // Should be DeviceNotFound or PermissionDenied or Io
        let msg = e.to_string();
        assert!(!msg.is_empty());
    }
}

// ============================================================================
// Platform-specific tests (marked as ignored)
// ============================================================================

#[test]
#[ignore] // Run with: cargo test -- --ignored test_unmount_device
fn test_unmount_device() {
    // This test requires an actual mounted device
    // Run manually after inserting a USB drive
}

#[test]
#[ignore]
fn test_open_real_device() {
    // This test requires root/admin and a real device
    // Example paths:
    // - Linux: /dev/sdb
    // - macOS: /dev/disk2
    // - Windows: \\.\PhysicalDrive1
}

#[test]
#[ignore]
fn test_direct_io_on_device() {
    // Direct I/O typically requires block devices, not regular files
    // This test needs a real device
}
