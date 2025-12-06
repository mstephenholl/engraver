//! Integration tests for engraver-core
//!
//! These tests verify the complete write pipeline using temporary files.

use engraver_core::{
    detect_source_type, format_duration, format_speed, get_source_size, validate_source, Error,
    Source, SourceInfo, SourceType, WriteConfig, WriteProgress, Writer, DEFAULT_BLOCK_SIZE,
    MAX_BLOCK_SIZE, MIN_BLOCK_SIZE,
};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tempfile::NamedTempFile;

// ============================================================================
// Writer integration tests
// ============================================================================

#[test]
fn test_write_small_file() {
    let source_data: Vec<u8> = (0..1024).map(|i| (i % 256) as u8).collect();
    let source = Cursor::new(source_data.clone());
    let mut target = Cursor::new(vec![0u8; 1024]);

    let config = WriteConfig::new().block_size(MIN_BLOCK_SIZE);
    let mut writer = Writer::with_config(config);

    let result = writer.write(source, &mut target, 1024).unwrap();

    assert_eq!(result.bytes_written, 1024);
    assert_eq!(result.retry_count, 0);

    // Verify data integrity
    target.seek(SeekFrom::Start(0)).unwrap();
    let written_data = target.into_inner();
    assert_eq!(written_data, source_data);
}

#[test]
fn test_write_large_file() {
    // 1 MB file
    let size = 1024 * 1024;
    let source_data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
    let source = Cursor::new(source_data.clone());
    let mut target = Cursor::new(vec![0u8; size]);

    let config = WriteConfig::new().block_size(64 * 1024); // 64 KB blocks
    let mut writer = Writer::with_config(config);

    let result = writer.write(source, &mut target, size as u64).unwrap();

    assert_eq!(result.bytes_written, size as u64);

    // Verify data
    let written_data = target.into_inner();
    assert_eq!(written_data, source_data);
}

#[test]
fn test_write_with_progress_tracking() {
    let size = 16 * 1024; // 16 KB
    let source_data = vec![0xABu8; size];
    let source = Cursor::new(source_data);
    let target = Cursor::new(vec![0u8; size]);

    let progress_updates = Arc::new(AtomicU64::new(0));
    let last_percentage = Arc::new(std::sync::Mutex::new(0.0f64));

    let progress_updates_clone = Arc::clone(&progress_updates);
    let last_percentage_clone = Arc::clone(&last_percentage);

    let config = WriteConfig::new().block_size(4096); // 4 KB blocks = 4 updates
    let mut writer = Writer::with_config(config).on_progress(move |progress| {
        progress_updates_clone.fetch_add(1, Ordering::SeqCst);
        let mut last = last_percentage_clone.lock().unwrap();
        // Percentage should be monotonically increasing
        assert!(
            progress.percentage() >= *last,
            "Progress went backwards: {} -> {}",
            *last,
            progress.percentage()
        );
        *last = progress.percentage();
    });

    let result = writer.write(source, target, size as u64).unwrap();

    assert_eq!(result.bytes_written, size as u64);
    assert_eq!(progress_updates.load(Ordering::SeqCst), 4); // 4 blocks
}

#[test]
fn test_write_cancellation() {
    let size = 1024 * 1024; // 1 MB
    let source_data = vec![0xABu8; size];
    let source = Cursor::new(source_data);
    let target = Cursor::new(vec![0u8; size]);

    let config = WriteConfig::new().block_size(4096);
    let writer = Writer::with_config(config);

    let cancel_handle = writer.cancel_handle();

    // Set up progress callback to cancel after first block
    let cancel = Arc::clone(&cancel_handle);
    let writer = writer.on_progress(move |progress| {
        if progress.current_block >= 1 {
            cancel.store(true, Ordering::SeqCst);
        }
    });

    let mut writer = writer;
    let result = writer.write(source, target, size as u64);

    assert!(matches!(result, Err(Error::Cancelled)));
}

#[test]
fn test_write_empty_source() {
    let source = Cursor::new(Vec::<u8>::new());
    let target = Cursor::new(vec![0u8; 1024]);

    let mut writer = Writer::new();
    let result = writer.write(source, target, 0).unwrap();

    assert_eq!(result.bytes_written, 0);
}

#[test]
fn test_write_to_tempfile() {
    let source_data: Vec<u8> = (0..8192).map(|i| (i % 256) as u8).collect();
    let source = Cursor::new(source_data.clone());

    let mut target = NamedTempFile::new().unwrap();
    // Pre-allocate space
    target.write_all(&vec![0u8; 8192]).unwrap();
    target.seek(SeekFrom::Start(0)).unwrap();

    let config = WriteConfig::new().block_size(4096).sync_on_complete(true);
    let mut writer = Writer::with_config(config);

    let result = writer.write(source, &mut target, 8192).unwrap();

    assert_eq!(result.bytes_written, 8192);

    // Read back and verify
    target.seek(SeekFrom::Start(0)).unwrap();
    let mut read_back = vec![0u8; 8192];
    std::io::Read::read_exact(&mut target, &mut read_back).unwrap();
    assert_eq!(read_back, source_data);
}

#[test]
fn test_write_sync_each_block() {
    let source_data = vec![0xABu8; 8192];
    let source = Cursor::new(source_data);
    let target = Cursor::new(vec![0u8; 8192]);

    let config = WriteConfig::new()
        .block_size(4096)
        .sync_each_block(true)
        .sync_on_complete(true);

    let mut writer = Writer::with_config(config);
    let result = writer.write(source, target, 8192).unwrap();

    assert_eq!(result.bytes_written, 8192);
}

#[test]
fn test_write_various_block_sizes() {
    let sizes = [MIN_BLOCK_SIZE, 8192, 16384, 65536, 1024 * 1024];

    for block_size in sizes {
        let data_size = block_size * 3; // 3 blocks
        let source_data: Vec<u8> = (0..data_size).map(|i| (i % 256) as u8).collect();
        let source = Cursor::new(source_data.clone());
        let mut target = Cursor::new(vec![0u8; data_size]);

        let config = WriteConfig::new().block_size(block_size);
        let mut writer = Writer::with_config(config);

        let result = writer.write(source, &mut target, data_size as u64).unwrap();

        assert_eq!(
            result.bytes_written, data_size as u64,
            "Failed for block_size {}",
            block_size
        );

        let written = target.into_inner();
        assert_eq!(written, source_data, "Data mismatch for block_size {}", block_size);
    }
}

#[test]
fn test_write_unaligned_size() {
    // Size that doesn't align to block size
    let size = 10000; // Not a multiple of 4096
    let source_data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
    let source = Cursor::new(source_data.clone());
    let mut target = Cursor::new(vec![0u8; size]);

    let config = WriteConfig::new().block_size(4096);
    let mut writer = Writer::with_config(config);

    let result = writer.write(source, &mut target, size as u64).unwrap();

    assert_eq!(result.bytes_written, size as u64);

    let written = target.into_inner();
    assert_eq!(written, source_data);
}

// ============================================================================
// WriteProgress tests
// ============================================================================

#[test]
fn test_write_progress_calculations() {
    let mut progress = WriteProgress::new(1000, 100);

    assert_eq!(progress.total_blocks, 10);
    assert_eq!(progress.percentage(), 0.0);
    assert!(!progress.is_complete());

    progress.bytes_written = 500;
    assert_eq!(progress.percentage(), 50.0);

    progress.bytes_written = 1000;
    assert!(progress.is_complete());
    assert_eq!(progress.percentage(), 100.0);
}

#[test]
fn test_write_progress_eta() {
    let mut progress = WriteProgress::new(1000, 100);
    progress.bytes_written = 500;
    progress.speed_bps = 100;
    progress.eta_seconds = Some(5);

    assert_eq!(progress.eta_display(), "5s");
}

// ============================================================================
// WriteConfig tests
// ============================================================================

#[test]
fn test_write_config_block_size_bounds() {
    // Test minimum clamping
    let config = WriteConfig::new().block_size(100);
    assert_eq!(config.block_size, MIN_BLOCK_SIZE);

    // Test maximum clamping
    let config = WriteConfig::new().block_size(1024 * 1024 * 1024);
    assert_eq!(config.block_size, MAX_BLOCK_SIZE);

    // Test valid size
    let config = WriteConfig::new().block_size(1024 * 1024);
    assert_eq!(config.block_size, 1024 * 1024);
}

#[test]
fn test_write_config_all_options() {
    let config = WriteConfig::new()
        .block_size(8192)
        .sync_each_block(true)
        .sync_on_complete(false)
        .retry_attempts(5)
        .retry_delay(Duration::from_millis(200))
        .verify(true);

    assert_eq!(config.block_size, 8192);
    assert!(config.sync_each_block);
    assert!(!config.sync_on_complete);
    assert_eq!(config.retry_attempts, 5);
    assert_eq!(config.retry_delay, Duration::from_millis(200));
    assert!(config.verify);
}

// ============================================================================
// Format function tests
// ============================================================================

#[test]
fn test_format_speed_ranges() {
    assert_eq!(format_speed(0), "0 B/s");
    assert_eq!(format_speed(1), "1 B/s");
    assert_eq!(format_speed(1023), "1023 B/s");
    assert_eq!(format_speed(1024), "1.0 KB/s");
    assert_eq!(format_speed(1024 * 1024 - 1), "1024.0 KB/s");
    assert_eq!(format_speed(1024 * 1024), "1.0 MB/s");
    assert_eq!(format_speed(1024 * 1024 * 1024), "1.0 GB/s");
}

#[test]
fn test_format_speed_realistic_values() {
    // Typical USB 2.0 speed
    assert_eq!(format_speed(30 * 1024 * 1024), "30.0 MB/s");

    // Typical USB 3.0 speed
    assert_eq!(format_speed(100 * 1024 * 1024), "100.0 MB/s");

    // NVMe speed
    assert_eq!(format_speed(3_500_000_000), "3.3 GB/s");
}

#[test]
fn test_format_duration_ranges() {
    assert_eq!(format_duration(0), "0s");
    assert_eq!(format_duration(59), "59s");
    assert_eq!(format_duration(60), "1m 0s");
    assert_eq!(format_duration(61), "1m 1s");
    assert_eq!(format_duration(3599), "59m 59s");
    assert_eq!(format_duration(3600), "1h 0m");
    assert_eq!(format_duration(3661), "1h 1m");
    assert_eq!(format_duration(7200), "2h 0m");
}

// ============================================================================
// Error handling tests
// ============================================================================

#[test]
fn test_error_display() {
    let err = Error::Cancelled;
    assert_eq!(err.to_string(), "Operation cancelled");

    let err = Error::PartialWrite {
        expected: 4096,
        actual: 1024,
    };
    assert!(err.to_string().contains("4096"));
    assert!(err.to_string().contains("1024"));
}

// ============================================================================
// Constants tests
// ============================================================================

#[test]
fn test_block_size_constants() {
    assert_eq!(MIN_BLOCK_SIZE, 4 * 1024);
    assert_eq!(DEFAULT_BLOCK_SIZE, 4 * 1024 * 1024);
    assert_eq!(MAX_BLOCK_SIZE, 64 * 1024 * 1024);

    // Ensure proper ordering
    assert!(MIN_BLOCK_SIZE < DEFAULT_BLOCK_SIZE);
    assert!(DEFAULT_BLOCK_SIZE < MAX_BLOCK_SIZE);
}

// ============================================================================
// Source integration tests
// ============================================================================

#[test]
fn test_source_type_detection_comprehensive() {
    // Local files
    assert_eq!(detect_source_type("image.iso"), SourceType::LocalFile);
    assert_eq!(detect_source_type("disk.img"), SourceType::LocalFile);
    assert_eq!(detect_source_type("/path/to/file"), SourceType::LocalFile);
    assert_eq!(detect_source_type("../relative/path.raw"), SourceType::LocalFile);

    // Compressed
    assert_eq!(detect_source_type("image.iso.gz"), SourceType::Gzip);
    assert_eq!(detect_source_type("image.iso.xz"), SourceType::Xz);
    assert_eq!(detect_source_type("image.iso.zst"), SourceType::Zstd);
    assert_eq!(detect_source_type("image.iso.bz2"), SourceType::Bzip2);

    // Remote
    assert_eq!(detect_source_type("http://example.com/file"), SourceType::Remote);
    assert_eq!(detect_source_type("https://example.com/file"), SourceType::Remote);
}

#[test]
fn test_source_open_and_read() {
    // Create a test file
    let mut temp = NamedTempFile::new().unwrap();
    let test_data = b"Test data for source reading integration test";
    temp.write_all(test_data).unwrap();
    temp.flush().unwrap();

    // Open via Source
    let mut source = Source::open(temp.path().to_str().unwrap()).unwrap();

    // Check info
    let info = source.info();
    assert_eq!(info.source_type, SourceType::LocalFile);
    assert_eq!(info.size, Some(test_data.len() as u64));
    assert!(info.seekable);

    // Read data
    let mut buffer = Vec::new();
    source.read_to_end(&mut buffer).unwrap();
    assert_eq!(buffer, test_data);
}

#[test]
fn test_source_size_detection() {
    let mut temp = NamedTempFile::new().unwrap();
    let data = vec![0u8; 4096];
    temp.write_all(&data).unwrap();

    let size = get_source_size(temp.path().to_str().unwrap()).unwrap();
    assert_eq!(size, Some(4096));
}

#[test]
fn test_source_validation() {
    let mut temp = NamedTempFile::new().unwrap();
    temp.write_all(&[0u8; 8192]).unwrap();

    let info = validate_source(temp.path().to_str().unwrap()).unwrap();
    assert_eq!(info.size, Some(8192));
    assert_eq!(info.source_type, SourceType::LocalFile);
}

#[test]
fn test_source_not_found() {
    let result = Source::open("/nonexistent/path/to/file.iso");
    assert!(matches!(result, Err(Error::SourceNotFound(_))));
}

#[test]
fn test_validate_source_not_found() {
    let result = validate_source("/nonexistent/path/to/file.iso");
    assert!(matches!(result, Err(Error::SourceNotFound(_))));
}

#[test]
fn test_source_type_properties() {
    assert!(!SourceType::LocalFile.is_compressed());
    assert!(!SourceType::Remote.is_compressed());
    assert!(SourceType::Gzip.is_compressed());
    assert!(SourceType::Xz.is_compressed());
    assert!(SourceType::Zstd.is_compressed());
    assert!(SourceType::Bzip2.is_compressed());

    assert!(!SourceType::LocalFile.is_remote());
    assert!(SourceType::Remote.is_remote());
}

#[test]
fn test_source_info_creation() {
    let local_info = SourceInfo::local("/path/to/file.iso", 1024 * 1024);
    assert_eq!(local_info.path, "/path/to/file.iso");
    assert_eq!(local_info.size, Some(1024 * 1024));
    assert!(local_info.seekable);
}

// ============================================================================
// Source + Writer pipeline test
// ============================================================================

#[test]
fn test_source_to_writer_pipeline() {
    // Create source file
    let mut source_file = NamedTempFile::new().unwrap();
    let source_data: Vec<u8> = (0..8192).map(|i| (i % 256) as u8).collect();
    source_file.write_all(&source_data).unwrap();
    source_file.flush().unwrap();

    // Open source
    let mut source = Source::open(source_file.path().to_str().unwrap()).unwrap();
    let source_size = source.size().unwrap();

    // Create target
    let mut target = Cursor::new(vec![0u8; 8192]);

    // Write using writer
    let config = WriteConfig::new().block_size(MIN_BLOCK_SIZE);
    let mut writer = Writer::with_config(config);

    let result = writer.write(&mut source, &mut target, source_size).unwrap();

    assert_eq!(result.bytes_written, 8192);

    // Verify data
    let written = target.into_inner();
    assert_eq!(written, source_data);
}

// ============================================================================
// Compression tests (require compression feature)
// ============================================================================

#[cfg(feature = "compression")]
mod compression_tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn test_gzip_source_pipeline() {
        use flate2::write::GzEncoder;
        use flate2::Compression;

        // Create compressed source
        let temp = NamedTempFile::new().unwrap();
        let path = temp.path().to_str().unwrap().to_string() + ".gz";

        let original_data = b"Hello from gzip compression test!";
        let file = File::create(&path).unwrap();
        let mut encoder = GzEncoder::new(file, Compression::default());
        encoder.write_all(original_data).unwrap();
        encoder.finish().unwrap();

        // Open and read
        let mut source = Source::open(&path).unwrap();
        assert!(source.is_compressed());
        assert!(!source.is_seekable());

        let mut buffer = Vec::new();
        source.read_to_end(&mut buffer).unwrap();
        assert_eq!(buffer, original_data);

        // Cleanup
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn test_xz_source_pipeline() {
        use xz2::write::XzEncoder;

        // Create compressed source
        let temp = NamedTempFile::new().unwrap();
        let path = temp.path().to_str().unwrap().to_string() + ".xz";

        let original_data = b"Hello from xz compression test!";
        let file = File::create(&path).unwrap();
        let mut encoder = XzEncoder::new(file, 6);
        encoder.write_all(original_data).unwrap();
        encoder.finish().unwrap();

        // Open and read
        let mut source = Source::open(&path).unwrap();
        assert!(source.is_compressed());

        let mut buffer = Vec::new();
        source.read_to_end(&mut buffer).unwrap();
        assert_eq!(buffer, original_data);

        // Cleanup
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn test_compressed_source_to_writer() {
        use flate2::write::GzEncoder;
        use flate2::Compression;

        // Create compressed source with enough data for multiple blocks
        let temp = NamedTempFile::new().unwrap();
        let path = temp.path().to_str().unwrap().to_string() + ".gz";

        let original_data: Vec<u8> = (0..MIN_BLOCK_SIZE * 2).map(|i| (i % 256) as u8).collect();
        let file = File::create(&path).unwrap();
        let mut encoder = GzEncoder::new(file, Compression::fast());
        encoder.write_all(&original_data).unwrap();
        encoder.finish().unwrap();

        // Open source
        let mut source = Source::open(&path).unwrap();

        // Create target (size unknown for compressed, use original size)
        let mut target = Cursor::new(vec![0u8; original_data.len()]);

        // Write
        let config = WriteConfig::new().block_size(MIN_BLOCK_SIZE);
        let mut writer = Writer::with_config(config);

        let result = writer
            .write(&mut source, &mut target, original_data.len() as u64)
            .unwrap();

        assert_eq!(result.bytes_written, original_data.len() as u64);

        // Verify
        let written = target.into_inner();
        assert_eq!(written, original_data);

        // Cleanup
        std::fs::remove_file(&path).unwrap();
    }
}
