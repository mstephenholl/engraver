//! Integration tests for actual write operations
//!
//! Tests the Writer against real temporary files (simulating block devices),
//! exercising file I/O paths that in-memory Cursor tests don't cover.

use engraver_core::{
    ChecksumAlgorithm, WriteConfig, WritePhase, WriteProgress, Writer, DEFAULT_BLOCK_SIZE,
    MAX_BLOCK_SIZE, MIN_BLOCK_SIZE,
};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tempfile::NamedTempFile;

// ============================================================================
// Helpers
// ============================================================================

/// Create a temp file pre-allocated to `size` bytes (simulates a block device).
fn create_test_device(size: u64) -> NamedTempFile {
    let file = NamedTempFile::new().unwrap();
    file.as_file().set_len(size).unwrap();
    file
}

/// Generate deterministic test data of given size.
fn test_data(size: usize) -> Vec<u8> {
    (0..size).map(|i| (i % 251) as u8).collect()
}

/// Read entire contents of a file from the beginning.
fn read_all(file: &mut std::fs::File) -> Vec<u8> {
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();
    buf
}

// ============================================================================
// Basic write operations to file devices
// ============================================================================

#[test]
fn write_to_file_device_small() {
    let data = test_data(64 * 1024); // 64 KB
    let source = Cursor::new(data.clone());
    let mut device = create_test_device(data.len() as u64);

    let config = WriteConfig::new().block_size(MIN_BLOCK_SIZE);
    let mut writer = Writer::with_config(config);

    let result = writer
        .write(source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    assert_eq!(result.bytes_written, data.len() as u64);
    assert_eq!(result.retry_count, 0);

    let written = read_all(device.as_file_mut());
    assert_eq!(written, data);
}

#[test]
fn write_to_file_device_large() {
    let data = test_data(8 * 1024 * 1024); // 8 MB
    let source = Cursor::new(data.clone());
    let mut device = create_test_device(data.len() as u64);

    let config = WriteConfig::new().block_size(DEFAULT_BLOCK_SIZE);
    let mut writer = Writer::with_config(config);

    let result = writer
        .write(source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    assert_eq!(result.bytes_written, data.len() as u64);

    let written = read_all(device.as_file_mut());
    assert_eq!(written, data);
}

#[test]
fn write_to_file_device_unaligned_size() {
    // Size not aligned to any common block size
    let data = test_data(1024 * 1024 + 37); // 1 MB + 37 bytes
    let source = Cursor::new(data.clone());
    let mut device = create_test_device(data.len() as u64);

    let config = WriteConfig::new().block_size(64 * 1024);
    let mut writer = Writer::with_config(config);

    let result = writer
        .write(source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    assert_eq!(result.bytes_written, data.len() as u64);

    let written = read_all(device.as_file_mut());
    assert_eq!(written, data);
}

// ============================================================================
// Various block sizes
// ============================================================================

#[test]
fn write_with_min_block_size() {
    let data = test_data(32 * 1024); // 32 KB = 8 blocks at 4KB
    let source = Cursor::new(data.clone());
    let mut device = create_test_device(data.len() as u64);

    let config = WriteConfig::new().block_size(MIN_BLOCK_SIZE);
    let mut writer = Writer::with_config(config);

    let result = writer
        .write(source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    assert_eq!(result.bytes_written, data.len() as u64);
    assert_eq!(read_all(device.as_file_mut()), data);
}

#[test]
fn write_with_64kb_block_size() {
    let data = test_data(512 * 1024); // 512 KB
    let source = Cursor::new(data.clone());
    let mut device = create_test_device(data.len() as u64);

    let config = WriteConfig::new().block_size(64 * 1024);
    let mut writer = Writer::with_config(config);

    let result = writer
        .write(source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    assert_eq!(result.bytes_written, data.len() as u64);
    assert_eq!(read_all(device.as_file_mut()), data);
}

#[test]
fn write_with_1mb_block_size() {
    let data = test_data(4 * 1024 * 1024); // 4 MB
    let source = Cursor::new(data.clone());
    let mut device = create_test_device(data.len() as u64);

    let config = WriteConfig::new().block_size(1024 * 1024);
    let mut writer = Writer::with_config(config);

    let result = writer
        .write(source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    assert_eq!(result.bytes_written, data.len() as u64);
    assert_eq!(read_all(device.as_file_mut()), data);
}

#[test]
fn write_with_max_block_size() {
    // Source smaller than MAX_BLOCK_SIZE to keep test fast
    let data = test_data(2 * 1024 * 1024); // 2 MB (single partial block at 64 MB)
    let source = Cursor::new(data.clone());
    let mut device = create_test_device(data.len() as u64);

    let config = WriteConfig::new().block_size(MAX_BLOCK_SIZE);
    let mut writer = Writer::with_config(config);

    let result = writer
        .write(source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    assert_eq!(result.bytes_written, data.len() as u64);
    assert_eq!(read_all(device.as_file_mut()), data);
}

// ============================================================================
// Resume (write_from_offset)
// ============================================================================

#[test]
fn write_from_offset_resume() {
    let data = test_data(1024 * 1024); // 1 MB total
    let half = data.len() / 2;

    let mut device = create_test_device(data.len() as u64);

    // Write first half
    let first_half = Cursor::new(data[..half].to_vec());
    let config = WriteConfig::new().block_size(64 * 1024);
    let mut writer = Writer::with_config(config.clone());
    writer
        .write(first_half, device.as_file_mut(), half as u64)
        .unwrap();

    // Write second half via write_from_offset
    let second_half = Cursor::new(data[half..].to_vec());
    let mut writer = Writer::with_config(config);
    writer
        .write_from_offset(
            second_half,
            device.as_file_mut(),
            data.len() as u64,
            half as u64,
        )
        .unwrap();

    // Verify full file
    let written = read_all(device.as_file_mut());
    assert_eq!(&written[..data.len()], &data[..]);
}

// ============================================================================
// Sync options
// ============================================================================

#[test]
fn write_with_sync_each_block() {
    let data = test_data(32 * 1024);
    let source = Cursor::new(data.clone());
    let mut device = create_test_device(data.len() as u64);

    let config = WriteConfig::new()
        .block_size(MIN_BLOCK_SIZE)
        .sync_each_block(true)
        .sync_on_complete(true);
    let mut writer = Writer::with_config(config);

    let result = writer
        .write(source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    assert_eq!(result.bytes_written, data.len() as u64);
    assert_eq!(read_all(device.as_file_mut()), data);
}

#[test]
fn write_with_no_sync() {
    let data = test_data(32 * 1024);
    let source = Cursor::new(data.clone());
    let mut device = create_test_device(data.len() as u64);

    let config = WriteConfig::new()
        .block_size(MIN_BLOCK_SIZE)
        .sync_each_block(false)
        .sync_on_complete(false);
    let mut writer = Writer::with_config(config);

    let result = writer
        .write(source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    assert_eq!(result.bytes_written, data.len() as u64);
    assert_eq!(read_all(device.as_file_mut()), data);
}

// ============================================================================
// Progress tracking
// ============================================================================

#[test]
fn write_progress_reports_writing_phase() {
    let data = test_data(32 * 1024); // 32 KB = 8 blocks at 4KB
    let source = Cursor::new(data.clone());
    let mut device = create_test_device(data.len() as u64);

    let phases = Arc::new(std::sync::Mutex::new(Vec::new()));
    let phases_clone = Arc::clone(&phases);

    let config = WriteConfig::new().block_size(MIN_BLOCK_SIZE);
    let mut writer = Writer::with_config(config).on_progress(move |p: &WriteProgress| {
        phases_clone.lock().unwrap().push(p.phase);
    });

    writer
        .write(source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    let phases = phases.lock().unwrap();
    assert!(!phases.is_empty(), "Should have received progress updates");
    assert!(
        phases.iter().all(|p| *p == WritePhase::Writing),
        "All phases should be Writing for plain write"
    );
}

#[test]
fn write_progress_percentage_monotonically_increases() {
    let data = test_data(64 * 1024);
    let source = Cursor::new(data.clone());
    let mut device = create_test_device(data.len() as u64);

    let percentages = Arc::new(std::sync::Mutex::new(Vec::new()));
    let percentages_clone = Arc::clone(&percentages);

    let config = WriteConfig::new().block_size(MIN_BLOCK_SIZE);
    let mut writer = Writer::with_config(config).on_progress(move |p: &WriteProgress| {
        percentages_clone.lock().unwrap().push(p.percentage());
    });

    writer
        .write(source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    let pcts = percentages.lock().unwrap();
    for window in pcts.windows(2) {
        assert!(
            window[1] >= window[0],
            "Progress went backwards: {} -> {}",
            window[0],
            window[1]
        );
    }
    // Last should be 100%
    assert!(
        (*pcts.last().unwrap() - 100.0).abs() < f64::EPSILON,
        "Final progress should be 100%"
    );
}

// ============================================================================
// Cancellation
// ============================================================================

#[test]
fn write_cancellation_mid_stream() {
    let data = test_data(1024 * 1024); // 1 MB
    let source = Cursor::new(data);
    let mut device = create_test_device(1024 * 1024);

    let config = WriteConfig::new().block_size(MIN_BLOCK_SIZE); // 4 KB blocks = 256 blocks
    let writer = Writer::with_config(config);
    let cancel = writer.cancel_handle();

    let bytes_written = Arc::new(AtomicU64::new(0));
    let bytes_clone = Arc::clone(&bytes_written);
    let cancel_clone = cancel.clone();

    let mut writer = writer.on_progress(move |p: &WriteProgress| {
        bytes_clone.store(p.bytes_written, Ordering::SeqCst);
        // Cancel after writing ~25% (64 blocks)
        if p.current_block >= 64 {
            cancel_clone.store(true, Ordering::SeqCst);
        }
    });

    let result = writer.write(source, device.as_file_mut(), 1024 * 1024);

    assert!(result.is_err(), "Write should have been cancelled");
    let written = bytes_written.load(Ordering::SeqCst);
    assert!(
        written > 0 && written < 1024 * 1024,
        "Should have written partial data: {}",
        written
    );
}

// ============================================================================
// Device larger than source
// ============================================================================

#[test]
fn write_preserves_data_beyond_source() {
    let source_size = 256 * 1024; // 256 KB
    let device_size = 1024 * 1024; // 1 MB
    let data = test_data(source_size);

    let mut device = create_test_device(device_size as u64);

    // Fill device with a known pattern first
    let fill = vec![0xFFu8; device_size];
    device.as_file_mut().write_all(&fill).unwrap();
    device.as_file_mut().seek(SeekFrom::Start(0)).unwrap();

    let source = Cursor::new(data.clone());
    let config = WriteConfig::new().block_size(64 * 1024);
    let mut writer = Writer::with_config(config);

    writer
        .write(source, device.as_file_mut(), source_size as u64)
        .unwrap();

    let written = read_all(device.as_file_mut());
    // First 256 KB should be source data
    assert_eq!(&written[..source_size], &data[..]);
    // Remaining should still be 0xFF (untouched)
    assert!(
        written[source_size..].iter().all(|&b| b == 0xFF),
        "Data beyond source should be untouched"
    );
}

// ============================================================================
// File-based source (LocalFileSource path)
// ============================================================================

#[test]
fn write_from_file_source_to_file_device() {
    let data = test_data(128 * 1024);

    // Create a source temp file
    let mut source_file = NamedTempFile::new().unwrap();
    source_file.write_all(&data).unwrap();
    source_file.seek(SeekFrom::Start(0)).unwrap();

    let mut device = create_test_device(data.len() as u64);

    let config = WriteConfig::new().block_size(64 * 1024);
    let mut writer = Writer::with_config(config);

    let result = writer
        .write(
            source_file.as_file_mut(),
            device.as_file_mut(),
            data.len() as u64,
        )
        .unwrap();

    assert_eq!(result.bytes_written, data.len() as u64);
    assert_eq!(read_all(device.as_file_mut()), data);
}

// ============================================================================
// Random data (catches buffer reuse bugs)
// ============================================================================

#[test]
fn write_large_random_data() {
    use rand::Rng;

    let size = 4 * 1024 * 1024; // 4 MB
    let mut rng = rand::rng();
    let data: Vec<u8> = (0..size).map(|_| rng.random::<u8>()).collect();

    let source = Cursor::new(data.clone());
    let mut device = create_test_device(size as u64);

    let config = WriteConfig::new().block_size(1024 * 1024);
    let mut writer = Writer::with_config(config);

    let result = writer
        .write(source, device.as_file_mut(), size as u64)
        .unwrap();

    assert_eq!(result.bytes_written, size as u64);
    assert_eq!(read_all(device.as_file_mut()), data);
}

// ============================================================================
// Write + verify end-to-end (checksum feature)
// ============================================================================

#[cfg(feature = "checksum")]
#[test]
fn write_and_verify_end_to_end_sha256() {
    let data = test_data(512 * 1024); // 512 KB
    let source = Cursor::new(data.clone());
    let mut device = create_test_device(data.len() as u64);

    let config = WriteConfig::new()
        .block_size(64 * 1024)
        .verify(true)
        .checksum_algorithm(Some(ChecksumAlgorithm::Sha256));
    let mut writer = Writer::with_config(config);

    let result = writer
        .write_and_verify(source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    assert_eq!(result.bytes_written, data.len() as u64);
    assert_eq!(result.verified, Some(true));
    assert!(result.source_checksum.is_some());
    assert!(result.target_checksum.is_some());
    assert_eq!(result.source_checksum, result.target_checksum);
    assert!(result.verification_elapsed.is_some());
}

#[cfg(feature = "checksum")]
#[test]
fn write_and_verify_reports_both_phases() {
    let data = test_data(64 * 1024);
    let source = Cursor::new(data.clone());
    let mut device = create_test_device(data.len() as u64);

    let phases = Arc::new(std::sync::Mutex::new(Vec::new()));
    let phases_clone = Arc::clone(&phases);

    let config = WriteConfig::new()
        .block_size(MIN_BLOCK_SIZE)
        .checksum_algorithm(Some(ChecksumAlgorithm::Sha256));
    let mut writer = Writer::with_config(config).on_progress(move |p: &WriteProgress| {
        phases_clone.lock().unwrap().push(p.phase);
    });

    writer
        .write_and_verify(source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    let phases = phases.lock().unwrap();
    let has_writing = phases.contains(&WritePhase::Writing);
    let has_verifying = phases.contains(&WritePhase::Verifying);
    assert!(has_writing, "Should report Writing phase");
    assert!(has_verifying, "Should report Verifying phase");
}

#[cfg(feature = "checksum")]
#[test]
fn write_and_verify_all_algorithms() {
    for algorithm in ChecksumAlgorithm::all() {
        let data = test_data(64 * 1024);
        let source = Cursor::new(data.clone());
        let mut device = create_test_device(data.len() as u64);

        let config = WriteConfig::new()
            .block_size(MIN_BLOCK_SIZE)
            .checksum_algorithm(Some(*algorithm));
        let mut writer = Writer::with_config(config);

        let result = writer
            .write_and_verify(source, device.as_file_mut(), data.len() as u64)
            .unwrap();

        assert_eq!(
            result.verified,
            Some(true),
            "Verification failed for {:?}",
            algorithm
        );
        assert_eq!(
            result.source_checksum, result.target_checksum,
            "Checksum mismatch for {:?}",
            algorithm
        );
    }
}
