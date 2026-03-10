//! Integration tests for verify operations
//!
//! Tests the Verifier against real temporary files, exercising file I/O
//! code paths for both byte-comparison and checksum verification.

use engraver_core::{Verifier, VerifyConfig, WriteConfig, Writer, MIN_VERIFY_BLOCK_SIZE};
use std::io::{Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tempfile::NamedTempFile;

#[cfg(feature = "checksum")]
use engraver_core::{auto_detect_checksum, ChecksumAlgorithm};

// ============================================================================
// Helpers
// ============================================================================

fn create_test_device(size: u64) -> NamedTempFile {
    let file = NamedTempFile::new().unwrap();
    file.as_file().set_len(size).unwrap();
    file
}

fn test_data(size: usize) -> Vec<u8> {
    (0..size).map(|i| (i % 251) as u8).collect()
}

/// Write data to a temp file and return it (seeked to start).
fn temp_file_with_data(data: &[u8]) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(data).unwrap();
    file.seek(SeekFrom::Start(0)).unwrap();
    file
}

// ============================================================================
// Byte-for-byte comparison tests
// ============================================================================

#[test]
fn verify_matching_files() {
    let data = test_data(256 * 1024); // 256 KB
    let mut source = temp_file_with_data(&data);
    let mut target = temp_file_with_data(&data);

    let mut verifier = Verifier::new();
    let result = verifier
        .compare(
            source.as_file_mut(),
            target.as_file_mut(),
            data.len() as u64,
        )
        .unwrap();

    assert!(result.success);
    assert_eq!(result.bytes_verified, data.len() as u64);
    assert_eq!(result.mismatches, 0);
    assert!(result.first_mismatch_offset.is_none());
}

#[test]
fn verify_mismatched_files_at_start() {
    let data = test_data(64 * 1024);
    let mut bad_data = data.clone();
    bad_data[0] = bad_data[0].wrapping_add(1); // flip first byte

    let mut source = temp_file_with_data(&data);
    let mut target = temp_file_with_data(&bad_data);

    let mut verifier = Verifier::new();
    let result = verifier
        .compare(
            source.as_file_mut(),
            target.as_file_mut(),
            data.len() as u64,
        )
        .unwrap();

    assert!(!result.success);
    assert_eq!(result.first_mismatch_offset, Some(0));
    assert!(result.mismatches >= 1);
}

#[test]
fn verify_mismatched_files_at_specific_offset() {
    let data = test_data(64 * 1024);
    let mut bad_data = data.clone();
    let offset = 12345;
    bad_data[offset] = bad_data[offset].wrapping_add(1);

    let mut source = temp_file_with_data(&data);
    let mut target = temp_file_with_data(&bad_data);

    let config = VerifyConfig::new().block_size(MIN_VERIFY_BLOCK_SIZE);
    let mut verifier = Verifier::with_config(config);
    let result = verifier
        .compare(
            source.as_file_mut(),
            target.as_file_mut(),
            data.len() as u64,
        )
        .unwrap();

    assert!(!result.success);
    assert_eq!(result.first_mismatch_offset, Some(offset as u64));
}

#[test]
fn verify_mismatched_at_end() {
    let data = test_data(64 * 1024);
    let mut bad_data = data.clone();
    let last = bad_data.len() - 1;
    bad_data[last] = bad_data[last].wrapping_add(1);

    let mut source = temp_file_with_data(&data);
    let mut target = temp_file_with_data(&bad_data);

    let mut verifier = Verifier::new();
    let result = verifier
        .compare(
            source.as_file_mut(),
            target.as_file_mut(),
            data.len() as u64,
        )
        .unwrap();

    assert!(!result.success);
    assert_eq!(result.first_mismatch_offset, Some(last as u64));
}

#[test]
fn verify_stop_on_mismatch_false_counts_all() {
    let data = test_data(64 * 1024);
    let mut bad_data = data.clone();
    // Corrupt two different blocks (at 4KB boundaries with MIN block size)
    bad_data[100] = bad_data[100].wrapping_add(1);
    bad_data[MIN_VERIFY_BLOCK_SIZE + 100] = bad_data[MIN_VERIFY_BLOCK_SIZE + 100].wrapping_add(1);

    let mut source = temp_file_with_data(&data);
    let mut target = temp_file_with_data(&bad_data);

    let config = VerifyConfig::new()
        .block_size(MIN_VERIFY_BLOCK_SIZE)
        .stop_on_mismatch(false);
    let mut verifier = Verifier::with_config(config);
    let result = verifier
        .compare(
            source.as_file_mut(),
            target.as_file_mut(),
            data.len() as u64,
        )
        .unwrap();

    assert!(!result.success);
    assert!(
        result.mismatches >= 2,
        "Should count at least 2 mismatched blocks, got {}",
        result.mismatches
    );
    assert_eq!(result.first_mismatch_offset, Some(100));
}

// ============================================================================
// Write-then-verify pipeline
// ============================================================================

#[test]
fn verify_write_then_compare() {
    let data = test_data(512 * 1024); // 512 KB
    let source_data = data.clone();

    // Write to device
    let mut device = create_test_device(data.len() as u64);
    let config = WriteConfig::new().block_size(64 * 1024);
    let mut writer = Writer::with_config(config);
    writer
        .write(
            std::io::Cursor::new(data.clone()),
            device.as_file_mut(),
            data.len() as u64,
        )
        .unwrap();

    // Verify by comparing source and written device
    let mut source = temp_file_with_data(&source_data);
    let mut verifier = Verifier::new();
    let result = verifier
        .compare(
            source.as_file_mut(),
            device.as_file_mut(),
            source_data.len() as u64,
        )
        .unwrap();

    assert!(result.success);
    assert_eq!(result.bytes_verified, source_data.len() as u64);
}

// ============================================================================
// Various verify block sizes
// ============================================================================

#[test]
fn verify_with_various_block_sizes() {
    let data = test_data(256 * 1024);

    for block_size in [MIN_VERIFY_BLOCK_SIZE, 64 * 1024, 1024 * 1024] {
        let mut source = temp_file_with_data(&data);
        let mut target = temp_file_with_data(&data);

        let config = VerifyConfig::new().block_size(block_size);
        let mut verifier = Verifier::with_config(config);
        let result = verifier
            .compare(
                source.as_file_mut(),
                target.as_file_mut(),
                data.len() as u64,
            )
            .unwrap();

        assert!(
            result.success,
            "Verification failed with block_size={}",
            block_size
        );
    }
}

// ============================================================================
// Progress reporting
// ============================================================================

#[test]
fn verify_progress_reports_correct_bytes() {
    let data = test_data(128 * 1024);
    let mut source = temp_file_with_data(&data);
    let mut target = temp_file_with_data(&data);

    let last_bytes = Arc::new(AtomicU64::new(0));
    let last_bytes_clone = Arc::clone(&last_bytes);
    let update_count = Arc::new(AtomicU64::new(0));
    let update_count_clone = Arc::clone(&update_count);

    let config = VerifyConfig::new().block_size(MIN_VERIFY_BLOCK_SIZE);
    let mut verifier = Verifier::with_config(config).on_progress(move |p| {
        last_bytes_clone.store(p.bytes_processed, Ordering::SeqCst);
        update_count_clone.fetch_add(1, Ordering::SeqCst);
    });

    let result = verifier
        .compare(
            source.as_file_mut(),
            target.as_file_mut(),
            data.len() as u64,
        )
        .unwrap();

    assert!(result.success);
    assert!(
        update_count.load(Ordering::SeqCst) > 0,
        "Should have received progress updates"
    );
    assert_eq!(
        last_bytes.load(Ordering::SeqCst),
        data.len() as u64,
        "Final progress should equal total size"
    );
}

// ============================================================================
// Cancellation
// ============================================================================

#[test]
fn verify_cancellation() {
    let data = test_data(1024 * 1024); // 1 MB
    let mut source = temp_file_with_data(&data);
    let mut target = temp_file_with_data(&data);

    let config = VerifyConfig::new().block_size(MIN_VERIFY_BLOCK_SIZE);
    let verifier = Verifier::with_config(config);
    let cancel = verifier.cancel_handle();

    // Cancel after the first progress callback (compare() resets the flag on entry)
    let cancel_clone = cancel.clone();
    let mut verifier = verifier.on_progress(move |_p| {
        cancel_clone.store(true, Ordering::SeqCst);
    });

    let result = verifier.compare(
        source.as_file_mut(),
        target.as_file_mut(),
        data.len() as u64,
    );

    assert!(result.is_err(), "Verification should have been cancelled");
}

// ============================================================================
// Large file verification
// ============================================================================

#[test]
fn verify_large_file() {
    let data = test_data(8 * 1024 * 1024); // 8 MB
    let mut source = temp_file_with_data(&data);
    let mut target = temp_file_with_data(&data);

    let mut verifier = Verifier::new();
    let result = verifier
        .compare(
            source.as_file_mut(),
            target.as_file_mut(),
            data.len() as u64,
        )
        .unwrap();

    assert!(result.success);
    assert_eq!(result.bytes_verified, data.len() as u64);
}

// ============================================================================
// Checksum tests (feature-gated)
// ============================================================================

#[cfg(feature = "checksum")]
#[test]
fn verify_checksum_sha256_on_file() {
    let data = test_data(128 * 1024);
    let mut file = temp_file_with_data(&data);

    let mut verifier = Verifier::new();
    let checksum = verifier
        .calculate_checksum(
            file.as_file_mut(),
            ChecksumAlgorithm::Sha256,
            Some(data.len() as u64),
        )
        .unwrap();

    // Verify it's the correct length
    assert_eq!(checksum.to_hex().len(), 64);

    // Recalculate and verify consistency
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut verifier2 = Verifier::new();
    let checksum2 = verifier2
        .calculate_checksum(
            file.as_file_mut(),
            ChecksumAlgorithm::Sha256,
            Some(data.len() as u64),
        )
        .unwrap();

    assert!(checksum.matches(&checksum2));
}

#[cfg(feature = "checksum")]
#[test]
fn verify_checksum_all_algorithms_on_file() {
    let data = test_data(64 * 1024);

    for algorithm in ChecksumAlgorithm::all() {
        let mut file = temp_file_with_data(&data);
        let mut verifier = Verifier::new();
        let checksum = verifier
            .calculate_checksum(file.as_file_mut(), *algorithm, Some(data.len() as u64))
            .unwrap();

        assert_eq!(
            checksum.to_hex().len(),
            algorithm.hex_length(),
            "Checksum length mismatch for {:?}",
            algorithm
        );

        // Verify against itself
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut verifier2 = Verifier::new();
        let result = verifier2
            .verify_checksum(
                file.as_file_mut(),
                *algorithm,
                &checksum.to_hex(),
                Some(data.len() as u64),
            )
            .unwrap();

        assert!(result.success, "Verify failed for {:?}", algorithm);
    }
}

#[cfg(feature = "checksum")]
#[test]
fn verify_checksum_detects_corruption() {
    let data = test_data(64 * 1024);
    let mut file = temp_file_with_data(&data);

    // Calculate checksum of original data
    let mut verifier = Verifier::new();
    let checksum = verifier
        .calculate_checksum(
            file.as_file_mut(),
            ChecksumAlgorithm::Sha256,
            Some(data.len() as u64),
        )
        .unwrap();

    // Corrupt the file
    file.seek(SeekFrom::Start(1000)).unwrap();
    file.write_all(&[0xFF]).unwrap();
    file.seek(SeekFrom::Start(0)).unwrap();

    // Verify should fail
    let mut verifier2 = Verifier::new();
    let result = verifier2.verify_checksum(
        file.as_file_mut(),
        ChecksumAlgorithm::Sha256,
        &checksum.to_hex(),
        Some(data.len() as u64),
    );

    assert!(result.is_err(), "Checksum should detect corruption");
}

#[cfg(feature = "checksum")]
#[test]
fn verify_checksum_with_progress() {
    let data = test_data(256 * 1024);
    let mut file = temp_file_with_data(&data);

    let update_count = Arc::new(AtomicU64::new(0));
    let update_count_clone = Arc::clone(&update_count);

    let config = VerifyConfig::new().block_size(MIN_VERIFY_BLOCK_SIZE);
    let mut verifier = Verifier::with_config(config).on_progress(move |_p| {
        update_count_clone.fetch_add(1, Ordering::SeqCst);
    });

    let _checksum = verifier
        .calculate_checksum(
            file.as_file_mut(),
            ChecksumAlgorithm::Sha256,
            Some(data.len() as u64),
        )
        .unwrap();

    assert!(
        update_count.load(Ordering::SeqCst) > 0,
        "Should have received progress updates during checksum calculation"
    );
}

// ============================================================================
// Checksum file auto-detection
// ============================================================================

#[cfg(feature = "checksum")]
#[test]
fn verify_auto_detect_checksum_file() {
    let data = test_data(32 * 1024);
    let mut file = temp_file_with_data(&data);

    // Calculate the actual checksum
    let mut verifier = Verifier::new();
    let checksum = verifier
        .calculate_checksum(
            file.as_file_mut(),
            ChecksumAlgorithm::Sha256,
            Some(data.len() as u64),
        )
        .unwrap();

    // Create a .sha256 sidecar file next to it
    let source_path = file.path().to_path_buf();
    let sha_path = source_path.with_extension("sha256");
    let filename = source_path.file_name().unwrap().to_str().unwrap();
    std::fs::write(&sha_path, format!("{}  {}\n", checksum.to_hex(), filename)).unwrap();

    // auto_detect_checksum should find it
    let detected = auto_detect_checksum(source_path.to_str().unwrap());
    assert!(
        detected.is_some(),
        "Should auto-detect .sha256 sidecar file"
    );

    let detected = detected.unwrap();
    assert_eq!(detected.algorithm, ChecksumAlgorithm::Sha256);
    assert_eq!(detected.checksum, checksum.to_hex());

    // Cleanup
    let _ = std::fs::remove_file(&sha_path);
}

// ============================================================================
// Write + verify full pipeline
// ============================================================================

#[cfg(feature = "checksum")]
#[test]
fn verify_full_write_and_verify_pipeline() {
    use rand::Rng;

    let size = 1024 * 1024; // 1 MB
    let mut rng = rand::rng();
    let data: Vec<u8> = (0..size).map(|_| rng.random::<u8>()).collect();

    let source = std::io::Cursor::new(data.clone());
    let mut device = create_test_device(size as u64);

    // Write with parallel verification
    let config = WriteConfig::new()
        .block_size(64 * 1024)
        .checksum_algorithm(Some(ChecksumAlgorithm::Sha256));
    let mut writer = Writer::with_config(config);

    let result = writer
        .write_and_verify(source, device.as_file_mut(), size as u64)
        .unwrap();

    assert_eq!(result.bytes_written, size as u64);
    assert_eq!(result.verified, Some(true));

    // Also do a byte-for-byte comparison as a second check
    let mut source_file = temp_file_with_data(&data);
    let mut verifier = Verifier::new();
    let compare_result = verifier
        .compare(source_file.as_file_mut(), device.as_file_mut(), size as u64)
        .unwrap();

    assert!(compare_result.success);
}
