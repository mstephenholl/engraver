//! End-to-end resume integration tests.
//!
//! These tests cover the full source → checkpoint → validate → resume
//! flow against real on-disk source / device files, using only the
//! public engraver-core API. They close the long-standing TODO.md
//! entry "Integration tests for actual write operations" and lock in
//! the resume content-hash safeguard added in commit fd2115c.
//!
//! Each test creates two temp files: a `source.iso` holding the image
//! data and a `device.img` standing in for the block device. The
//! source's header hash is computed via the public
//! `compute_local_header_hash` helper, set on the `SourceInfo`, and
//! then carried into the `WriteCheckpoint` exactly the way the CLI's
//! `commands/write.rs` does it.

use engraver_core::{
    compute_local_header_hash, validate_checkpoint, CheckpointManager, SourceInfo, SourceType,
    WriteCheckpoint, WriteConfig, Writer,
};
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use tempfile::TempDir;

// =============================================================================
// Helpers
// =============================================================================

/// Produce deterministic, distinguishable image bytes: each image is
/// the same length but differs in every byte from any other (we vary
/// by the `tag` parameter).
fn image_bytes(size: usize, tag: u8) -> Vec<u8> {
    (0..size).map(|i| (i as u8).wrapping_add(tag)).collect()
}

/// Write `bytes` to `path`, creating or truncating.
fn write_file(path: &std::path::Path, bytes: &[u8]) {
    let mut f = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .unwrap();
    f.write_all(bytes).unwrap();
    f.sync_all().unwrap();
}

/// Slurp a file into memory for content comparison.
fn read_file(path: &std::path::Path) -> Vec<u8> {
    let mut f = std::fs::File::open(path).unwrap();
    let mut out = Vec::new();
    f.read_to_end(&mut out).unwrap();
    out
}

/// Build a SourceInfo for a local file, with the header hash already
/// populated — mirrors what `commands/write.rs::populate_source_header_hash`
/// does at the CLI layer.
fn local_source_info_with_hash(path: &std::path::Path, size: u64) -> SourceInfo {
    let mut info = SourceInfo {
        path: path.to_str().unwrap().to_string(),
        source_type: SourceType::LocalFile,
        compressed_size: Some(size),
        size: Some(size),
        seekable: true,
        resumable: false,
        content_type: None,
        etag: None,
        source_header_hash: None,
    };
    info.source_header_hash = compute_local_header_hash(info.path.as_str())
        .expect("hash compute should succeed on a real file");
    info
}

// =============================================================================
// 1. Full happy-path write
// =============================================================================

#[test]
fn full_write_then_device_contents_match_source() {
    let dir = TempDir::new().unwrap();
    let source_path = dir.path().join("source.iso");
    let device_path = dir.path().join("device.img");

    let size = 256 * 1024; // 256 KB — small but exercises multiple blocks
    let data = image_bytes(size, 0xAA);
    write_file(&source_path, &data);
    write_file(&device_path, &vec![0u8; size]); // pre-sized device

    let source = std::fs::File::open(&source_path).unwrap();
    let mut device = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&device_path)
        .unwrap();

    let mut writer = Writer::with_config(WriteConfig::new().block_size(64 * 1024));
    let result = writer.write(source, &mut device, size as u64).unwrap();

    assert_eq!(result.bytes_written, size as u64);
    assert_eq!(
        read_file(&device_path),
        data,
        "device must equal source byte-for-byte"
    );
}

// =============================================================================
// 2. Interrupt then resume — same source → completes the image
// =============================================================================

#[test]
fn interrupted_write_resumes_to_full_image_when_source_unchanged() {
    let dir = TempDir::new().unwrap();
    let source_path = dir.path().join("source.iso");
    let device_path = dir.path().join("device.img");

    let size = 256 * 1024;
    let block_size = 64 * 1024;
    let data = image_bytes(size, 0xAA);
    write_file(&source_path, &data);
    write_file(&device_path, &vec![0u8; size]);

    // --- First attempt: write only the first half, then "interrupt".
    let half = size / 2;
    let mut device = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&device_path)
        .unwrap();
    let half_source: Vec<u8> = data[..half].to_vec();
    let mut writer = Writer::with_config(WriteConfig::new().block_size(block_size));
    writer
        .write(std::io::Cursor::new(half_source), &mut device, half as u64)
        .unwrap();

    // Save a checkpoint reflecting the partial state.
    let info = local_source_info_with_hash(&source_path, size as u64);
    let cp_dir = dir.path().join("checkpoints");
    let mgr = CheckpointManager::new(&cp_dir).unwrap();
    let mut checkpoint = WriteCheckpoint::new(
        &info,
        device_path.to_str().unwrap(),
        size as u64,
        &WriteConfig::new().block_size(block_size),
    );
    checkpoint.update_progress(
        half as u64,
        (half / block_size) as u64,
        std::time::Duration::from_secs(1),
    );
    mgr.save(&checkpoint).unwrap();

    // --- Resume: validate against the unchanged source, then finish.
    let validation = validate_checkpoint(&checkpoint, &info, size as u64);
    assert!(
        validation.valid,
        "validation should pass: {:?}",
        validation.messages
    );

    let mut source = std::fs::File::open(&source_path).unwrap();
    source.seek(SeekFrom::Start(half as u64)).unwrap();
    let mut writer = Writer::with_config(WriteConfig::new().block_size(block_size));
    writer
        .write_from_offset(source, &mut device, size as u64, half as u64)
        .unwrap();

    assert_eq!(
        read_file(&device_path),
        data,
        "device must contain the original image after resume"
    );
}

// =============================================================================
// 3. Replaced source → resume is blocked, device left untouched
// =============================================================================

#[test]
fn resume_blocked_when_source_replaced_with_different_image() {
    let dir = TempDir::new().unwrap();
    let source_path = dir.path().join("source.iso");
    let device_path = dir.path().join("device.img");

    let size = 256 * 1024;
    let block_size = 64 * 1024;
    let image_a = image_bytes(size, 0xAA);
    let image_b = image_bytes(size, 0x55); // SAME SIZE, different bytes
    assert_ne!(image_a, image_b, "test setup: A and B must differ");

    write_file(&source_path, &image_a);
    write_file(&device_path, &vec![0u8; size]);

    // Partially write A.
    let half = size / 2;
    {
        let mut device = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&device_path)
            .unwrap();
        let mut writer = Writer::with_config(WriteConfig::new().block_size(block_size));
        writer
            .write(
                std::io::Cursor::new(image_a[..half].to_vec()),
                &mut device,
                half as u64,
            )
            .unwrap();
    }
    let device_after_partial_a = read_file(&device_path);

    // Snapshot the checkpoint with A's hash.
    let info_a = local_source_info_with_hash(&source_path, size as u64);
    let mgr = CheckpointManager::new(dir.path().join("checkpoints")).unwrap();
    let mut checkpoint = WriteCheckpoint::new(
        &info_a,
        device_path.to_str().unwrap(),
        size as u64,
        &WriteConfig::new().block_size(block_size),
    );
    checkpoint.update_progress(
        half as u64,
        (half / block_size) as u64,
        std::time::Duration::from_secs(1),
    );
    mgr.save(&checkpoint).unwrap();

    // User replaces the source file with image B — same path, same length.
    write_file(&source_path, &image_b);
    let info_b = local_source_info_with_hash(&source_path, size as u64);
    assert_ne!(
        info_a.source_header_hash, info_b.source_header_hash,
        "hashes must differ for the safeguard to mean anything"
    );

    // Validation must refuse to resume.
    let validation = validate_checkpoint(&checkpoint, &info_b, size as u64);
    assert!(
        !validation.valid,
        "validate_checkpoint must reject the replaced image"
    );
    assert!(
        validation
            .messages
            .iter()
            .any(|m| m.contains("content") || m.contains("hash")),
        "diagnostic should mention content / hash: {:?}",
        validation.messages
    );

    // Crucially: the device has NOT been touched since the partial A write.
    // No bytes of image B leaked onto the target.
    assert_eq!(
        read_file(&device_path),
        device_after_partial_a,
        "device must not have been modified during the rejected validation"
    );
}

// =============================================================================
// 4. Target-size mismatch is a warning, not an error
// =============================================================================

#[test]
fn target_size_mismatch_warns_but_does_not_block_resume() {
    let dir = TempDir::new().unwrap();
    let source_path = dir.path().join("source.iso");

    let size = 128 * 1024;
    write_file(&source_path, &image_bytes(size, 0xCC));

    let info = local_source_info_with_hash(&source_path, size as u64);
    let checkpoint = WriteCheckpoint::new(
        &info,
        "/dev/imaginary",
        4 * 1024 * 1024 * 1024, // original target was 4 GB
        &WriteConfig::new(),
    );

    // The user re-runs against a target whose size has changed (e.g.
    // they switched USB sticks). validate_checkpoint should warn but
    // permit the resume — only path / size / hash / etag mismatches
    // are fatal.
    let validation = validate_checkpoint(&checkpoint, &info, 8 * 1024 * 1024 * 1024);
    assert!(
        validation.valid,
        "target size mismatch must NOT invalidate the checkpoint"
    );
    assert!(
        !validation.warnings.is_empty(),
        "but it should produce a warning"
    );
    assert!(
        validation
            .warnings
            .iter()
            .any(|w| w.contains("Target size") || w.contains("target size")),
        "warning should mention target size: {:?}",
        validation.warnings
    );
}

// =============================================================================
// 5. CheckpointManager round-trip through disk preserves the hash
// =============================================================================

#[test]
fn checkpoint_manager_round_trip_preserves_header_hash_and_etag() {
    // Locks in the on-disk format compatibility: a checkpoint written
    // by one process must round-trip through the manager's save/load
    // with the safety fields intact.
    let dir = TempDir::new().unwrap();
    let source_path = dir.path().join("image.iso");
    write_file(&source_path, &image_bytes(64 * 1024, 0x33));

    let mut info = local_source_info_with_hash(&source_path, 64 * 1024);
    info.etag = Some("\"fake-etag\"".to_string());

    let mgr = CheckpointManager::new(dir.path().join("checkpoints")).unwrap();
    let original = WriteCheckpoint::new(&info, "/dev/test", 1024 * 1024, &WriteConfig::new());
    mgr.save(&original).unwrap();

    let loaded = mgr.load(&original).unwrap();
    assert_eq!(loaded.source_header_hash, original.source_header_hash);
    assert!(
        loaded.source_header_hash.is_some(),
        "hash must survive the round-trip"
    );
    assert_eq!(loaded.etag, original.etag);
    assert_eq!(loaded.etag.as_deref(), Some("\"fake-etag\""));
}
