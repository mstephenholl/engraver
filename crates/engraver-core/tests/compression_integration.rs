//! Integration tests for compression decompression with real compressed images
//!
//! Tests the full pipeline: compress test data → create source → decompress via
//! Writer → verify output matches original data. Feature-gated on `compression`.

#![cfg(feature = "compression")]

use engraver_core::{detect_source_type, Source, SourceType, WriteConfig, Writer, MIN_BLOCK_SIZE};
use std::io::{Read, Seek, SeekFrom, Write};
use tempfile::NamedTempFile;

// ============================================================================
// Helpers
// ============================================================================

fn test_data(size: usize) -> Vec<u8> {
    (0..size).map(|i| (i % 251) as u8).collect()
}

fn read_all(file: &mut std::fs::File) -> Vec<u8> {
    file.seek(SeekFrom::Start(0)).unwrap();
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).unwrap();
    buf
}

fn create_test_device(size: u64) -> NamedTempFile {
    let file = NamedTempFile::new().unwrap();
    file.as_file().set_len(size).unwrap();
    file
}

/// Create a gzip-compressed temp file from data. Returns (tempfile, path_str).
/// The file is named with .gz extension so source type detection works.
fn create_gzip_file(data: &[u8]) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.img.gz");
    let file = std::fs::File::create(&path).unwrap();
    let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap();
    let path_str = path.to_str().unwrap().to_string();
    (dir, path_str)
}

/// Create an xz-compressed temp file from data.
fn create_xz_file(data: &[u8]) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.img.xz");
    let file = std::fs::File::create(&path).unwrap();
    let mut encoder = xz2::write::XzEncoder::new(file, 1); // level 1 for speed
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap();
    let path_str = path.to_str().unwrap().to_string();
    (dir, path_str)
}

/// Create a zstd-compressed temp file from data.
fn create_zstd_file(data: &[u8]) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.img.zst");
    let file = std::fs::File::create(&path).unwrap();
    let mut encoder = zstd::Encoder::new(file, 1).unwrap(); // level 1 for speed
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap();
    let path_str = path.to_str().unwrap().to_string();
    (dir, path_str)
}

/// Create a bzip2-compressed temp file from data.
fn create_bzip2_file(data: &[u8]) -> (tempfile::TempDir, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("test.img.bz2");
    let file = std::fs::File::create(&path).unwrap();
    let mut encoder = bzip2::write::BzEncoder::new(file, bzip2::Compression::fast());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap();
    let path_str = path.to_str().unwrap().to_string();
    (dir, path_str)
}

// ============================================================================
// Source type detection by extension
// ============================================================================

#[test]
fn detect_gzip_by_extension() {
    assert_eq!(detect_source_type("image.iso.gz"), SourceType::Gzip);
}

#[test]
fn detect_xz_by_extension() {
    assert_eq!(detect_source_type("image.iso.xz"), SourceType::Xz);
}

#[test]
fn detect_zstd_by_extension() {
    assert_eq!(detect_source_type("image.iso.zst"), SourceType::Zstd);
}

#[test]
fn detect_bzip2_by_extension() {
    assert_eq!(detect_source_type("image.iso.bz2"), SourceType::Bzip2);
}

// ============================================================================
// Gzip pipeline tests
// ============================================================================

#[test]
fn gzip_decompress_write_pipeline() {
    let data = test_data(128 * 1024); // 128 KB
    let (_dir, path) = create_gzip_file(&data);

    let mut source = Source::open(&path).unwrap();
    let mut device = create_test_device(data.len() as u64);

    // We don't know decompressed size upfront for gzip
    let config = WriteConfig::new().block_size(MIN_BLOCK_SIZE);
    let mut writer = Writer::with_config(config);

    let result = writer
        .write(&mut source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    assert_eq!(result.bytes_written, data.len() as u64);
    assert_eq!(read_all(device.as_file_mut()), data);
}

#[test]
fn gzip_large_file() {
    let data = test_data(4 * 1024 * 1024); // 4 MB
    let (_dir, path) = create_gzip_file(&data);

    let mut source = Source::open(&path).unwrap();
    let mut device = create_test_device(data.len() as u64);

    let config = WriteConfig::new().block_size(1024 * 1024);
    let mut writer = Writer::with_config(config);

    let result = writer
        .write(&mut source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    assert_eq!(result.bytes_written, data.len() as u64);
    assert_eq!(read_all(device.as_file_mut()), data);
}

// ============================================================================
// XZ pipeline tests
// ============================================================================

#[test]
fn xz_decompress_write_pipeline() {
    let data = test_data(128 * 1024);
    let (_dir, path) = create_xz_file(&data);

    let mut source = Source::open(&path).unwrap();
    let mut device = create_test_device(data.len() as u64);

    let config = WriteConfig::new().block_size(MIN_BLOCK_SIZE);
    let mut writer = Writer::with_config(config);

    let result = writer
        .write(&mut source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    assert_eq!(result.bytes_written, data.len() as u64);
    assert_eq!(read_all(device.as_file_mut()), data);
}

// ============================================================================
// Zstd pipeline tests
// ============================================================================

#[test]
fn zstd_decompress_write_pipeline() {
    let data = test_data(128 * 1024);
    let (_dir, path) = create_zstd_file(&data);

    let mut source = Source::open(&path).unwrap();
    let mut device = create_test_device(data.len() as u64);

    let config = WriteConfig::new().block_size(MIN_BLOCK_SIZE);
    let mut writer = Writer::with_config(config);

    let result = writer
        .write(&mut source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    assert_eq!(result.bytes_written, data.len() as u64);
    assert_eq!(read_all(device.as_file_mut()), data);
}

// ============================================================================
// Bzip2 pipeline tests
// ============================================================================

#[test]
fn bzip2_decompress_write_pipeline() {
    let data = test_data(128 * 1024);
    let (_dir, path) = create_bzip2_file(&data);

    let mut source = Source::open(&path).unwrap();
    let mut device = create_test_device(data.len() as u64);

    let config = WriteConfig::new().block_size(MIN_BLOCK_SIZE);
    let mut writer = Writer::with_config(config);

    let result = writer
        .write(&mut source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    assert_eq!(result.bytes_written, data.len() as u64);
    assert_eq!(read_all(device.as_file_mut()), data);
}

// ============================================================================
// Source info for compressed files
// ============================================================================

#[test]
fn compressed_source_reports_compressed_type() {
    let data = test_data(32 * 1024);

    let (_dir_gz, path_gz) = create_gzip_file(&data);
    let source = Source::open(&path_gz).unwrap();
    assert!(source.is_compressed());

    let (_dir_xz, path_xz) = create_xz_file(&data);
    let source = Source::open(&path_xz).unwrap();
    assert!(source.is_compressed());

    let (_dir_zst, path_zst) = create_zstd_file(&data);
    let source = Source::open(&path_zst).unwrap();
    assert!(source.is_compressed());

    let (_dir_bz2, path_bz2) = create_bzip2_file(&data);
    let source = Source::open(&path_bz2).unwrap();
    assert!(source.is_compressed());
}

// ============================================================================
// Various block sizes with compressed sources
// ============================================================================

#[test]
fn compressed_write_with_min_block_size() {
    let data = test_data(64 * 1024);
    let (_dir, path) = create_gzip_file(&data);

    let mut source = Source::open(&path).unwrap();
    let mut device = create_test_device(data.len() as u64);

    let config = WriteConfig::new().block_size(MIN_BLOCK_SIZE);
    let mut writer = Writer::with_config(config);

    let result = writer
        .write(&mut source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    assert_eq!(result.bytes_written, data.len() as u64);
    assert_eq!(read_all(device.as_file_mut()), data);
}

#[test]
fn compressed_write_with_large_block_size() {
    let data = test_data(64 * 1024);
    let (_dir, path) = create_gzip_file(&data);

    let mut source = Source::open(&path).unwrap();
    let mut device = create_test_device(data.len() as u64);

    // Block size larger than data — should still work
    let config = WriteConfig::new().block_size(1024 * 1024);
    let mut writer = Writer::with_config(config);

    let result = writer
        .write(&mut source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    assert_eq!(result.bytes_written, data.len() as u64);
    assert_eq!(read_all(device.as_file_mut()), data);
}

// ============================================================================
// Compressed + write_and_verify pipeline
// ============================================================================

#[cfg(feature = "checksum")]
#[test]
fn compressed_write_and_verify() {
    use engraver_core::ChecksumAlgorithm;

    let data = test_data(256 * 1024);
    let (_dir, path) = create_gzip_file(&data);

    let mut source = Source::open(&path).unwrap();
    let mut device = create_test_device(data.len() as u64);

    let config = WriteConfig::new()
        .block_size(64 * 1024)
        .checksum_algorithm(Some(ChecksumAlgorithm::Sha256));
    let mut writer = Writer::with_config(config);

    let result = writer
        .write_and_verify(&mut source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    assert_eq!(result.bytes_written, data.len() as u64);
    assert_eq!(result.verified, Some(true));
    assert_eq!(read_all(device.as_file_mut()), data);
}

// ============================================================================
// Progress with compressed sources
// ============================================================================

#[test]
fn compressed_write_with_progress() {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    let data = test_data(128 * 1024);
    let (_dir, path) = create_gzip_file(&data);

    let mut source = Source::open(&path).unwrap();
    let mut device = create_test_device(data.len() as u64);

    let update_count = Arc::new(AtomicU64::new(0));
    let update_count_clone = Arc::clone(&update_count);

    let config = WriteConfig::new().block_size(MIN_BLOCK_SIZE);
    let mut writer = Writer::with_config(config).on_progress(move |_p| {
        update_count_clone.fetch_add(1, Ordering::SeqCst);
    });

    writer
        .write(&mut source, device.as_file_mut(), data.len() as u64)
        .unwrap();

    assert!(
        update_count.load(Ordering::SeqCst) > 0,
        "Should receive progress updates during compressed write"
    );
}

// ============================================================================
// Corrupted compressed files
// ============================================================================

#[test]
fn corrupted_gzip_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("corrupt.img.gz");

    // Write truncated gzip data (magic bytes + garbage)
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(&[0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0xFF, 0xFF])
        .unwrap();
    drop(file);

    let source = Source::open(path.to_str().unwrap());
    // Opening might succeed (only header is checked), but reading should fail
    if let Ok(mut source) = source {
        let mut device = create_test_device(1024);
        let config = WriteConfig::new().block_size(MIN_BLOCK_SIZE);
        let mut writer = Writer::with_config(config);

        let result = writer.write(&mut source, device.as_file_mut(), 1024);
        // Should error during decompression
        assert!(
            result.is_err() || {
                // If no error, the data should be short (truncated source)
                let written = read_all(device.as_file_mut());
                written.len() < 1024
            },
            "Corrupted gzip should fail or produce truncated output"
        );
    }
    // If Source::open fails, that's also acceptable
}

#[test]
fn corrupted_xz_returns_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("corrupt.img.xz");

    // XZ magic bytes + garbage
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(&[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00, 0xFF, 0xFF])
        .unwrap();
    drop(file);

    let source = Source::open(path.to_str().unwrap());
    if let Ok(mut source) = source {
        let mut device = create_test_device(1024);
        let config = WriteConfig::new().block_size(MIN_BLOCK_SIZE);
        let mut writer = Writer::with_config(config);

        let result = writer.write(&mut source, device.as_file_mut(), 1024);
        assert!(
            result.is_err() || {
                let written = read_all(device.as_file_mut());
                written.len() < 1024
            },
            "Corrupted xz should fail or produce truncated output"
        );
    }
}

// ============================================================================
// Empty compressed file
// ============================================================================

#[test]
fn empty_gzip_produces_empty_write() {
    let data: Vec<u8> = vec![];
    let (_dir, path) = create_gzip_file(&data);

    let mut source = Source::open(&path).unwrap();
    let mut device = create_test_device(1024);

    let config = WriteConfig::new().block_size(MIN_BLOCK_SIZE);
    let mut writer = Writer::with_config(config);

    let result = writer.write(&mut source, device.as_file_mut(), 0).unwrap();

    assert_eq!(result.bytes_written, 0);
}

// ============================================================================
// Magic byte detection
// ============================================================================

#[test]
fn detect_compression_from_real_files() {
    let data = test_data(1024);

    let (_dir_gz, path_gz) = create_gzip_file(&data);
    assert!(detect_source_type(&path_gz).is_compressed());

    let (_dir_xz, path_xz) = create_xz_file(&data);
    assert!(detect_source_type(&path_xz).is_compressed());

    let (_dir_zst, path_zst) = create_zstd_file(&data);
    assert!(detect_source_type(&path_zst).is_compressed());

    let (_dir_bz2, path_bz2) = create_bzip2_file(&data);
    assert!(detect_source_type(&path_bz2).is_compressed());
}

// ============================================================================
// All formats with larger data
// ============================================================================

#[test]
fn all_formats_4mb_roundtrip() {
    let data = test_data(4 * 1024 * 1024); // 4 MB

    type CreateFn = dyn Fn(&[u8]) -> (tempfile::TempDir, String);
    let formats: Vec<(&str, Box<CreateFn>)> = vec![
        ("gzip", Box::new(create_gzip_file)),
        ("xz", Box::new(create_xz_file)),
        ("zstd", Box::new(create_zstd_file)),
        ("bzip2", Box::new(create_bzip2_file)),
    ];

    for (name, create_fn) in &formats {
        let (_dir, path) = create_fn(&data);
        let mut source = Source::open(&path).unwrap();
        let mut device = create_test_device(data.len() as u64);

        let config = WriteConfig::new().block_size(1024 * 1024);
        let mut writer = Writer::with_config(config);

        let result = writer
            .write(&mut source, device.as_file_mut(), data.len() as u64)
            .unwrap();

        assert_eq!(
            result.bytes_written,
            data.len() as u64,
            "{} write size mismatch",
            name
        );
        assert_eq!(
            read_all(device.as_file_mut()),
            data,
            "{} data mismatch",
            name
        );
    }
}
