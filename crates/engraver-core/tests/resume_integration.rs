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

use engraver_core::resume::TargetIdentity;
use engraver_core::{
    compute_local_header_hash, validate_checkpoint, CheckpointManager, SourceInfo, SourceType,
    WriteCheckpoint, WriteConfig, Writer, CHECKPOINT_VERSION,
};
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use tempfile::TempDir;

// =============================================================================
// Helpers
// =============================================================================

/// A target that is known only by its size, as a drive with no serial.
fn target(size: u64) -> TargetIdentity {
    TargetIdentity {
        size,
        serial: None,
        model: None,
    }
}

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
        min_size: None,
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
        &target(size as u64),
        &WriteConfig::new().block_size(block_size),
    );
    checkpoint.update_progress(
        half as u64,
        (half / block_size) as u64,
        std::time::Duration::from_secs(1),
    );
    mgr.save(&checkpoint).unwrap();

    // --- Resume: validate against the unchanged source, then finish.
    let validation = validate_checkpoint(&checkpoint, &info, &target(size as u64));
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
        &target(size as u64),
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
    let validation = validate_checkpoint(&checkpoint, &info_b, &target(size as u64));
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
// 4. A resume must be for the drive the checkpoint was made for
// =============================================================================

/// A drive with a serial and a model, as the platform layer reports one.
fn drive(size: u64, serial: Option<&str>, model: Option<&str>) -> TargetIdentity {
    TargetIdentity {
        size,
        serial: serial.map(str::to_string),
        model: model.map(str::to_string),
    }
}

/// A checkpoint made for `made_for`, and the source it was made from.
fn checkpoint_for(dir: &TempDir, made_for: &TargetIdentity) -> (WriteCheckpoint, SourceInfo) {
    let source_path = dir.path().join("source.iso");
    let size = 128 * 1024;
    write_file(&source_path, &image_bytes(size, 0xCC));

    let info = local_source_info_with_hash(&source_path, size as u64);
    let checkpoint = WriteCheckpoint::new(&info, "/dev/imaginary", made_for, &WriteConfig::new());
    (checkpoint, info)
}

#[test]
fn target_size_mismatch_invalidates_checkpoint() {
    let dir = TempDir::new().unwrap();
    let four_gb = 4 * 1024 * 1024 * 1024;
    let eight_gb = 8 * 1024 * 1024 * 1024;
    let (checkpoint, info) = checkpoint_for(&dir, &target(four_gb));

    // A different stick shows up at the same path. Resuming would write the
    // tail of the image onto it and leave its first part never written.
    let validation = validate_checkpoint(&checkpoint, &info, &target(eight_gb));

    assert!(
        !validation.valid,
        "a drive of another size must not be resumed onto"
    );
    let message = validation.messages.join("\n");
    assert!(
        message.contains("size")
            && message.contains("4294967296")
            && message.contains("8589934592"),
        "message should name the size and both values: {message}"
    );
}

#[test]
fn target_serial_mismatch_invalidates_checkpoint() {
    let dir = TempDir::new().unwrap();
    let size = 4 * 1024 * 1024 * 1024;
    let (checkpoint, info) = checkpoint_for(&dir, &drive(size, Some("SER-AAAA"), Some("Flash")));

    // Same size, same model, same path, another physical stick.
    let validation = validate_checkpoint(
        &checkpoint,
        &info,
        &drive(size, Some("SER-BBBB"), Some("Flash")),
    );

    assert!(
        !validation.valid,
        "a drive with another serial must not be resumed onto"
    );
    let message = validation.messages.join("\n");
    assert!(
        message.contains("SER-AAAA") && message.contains("SER-BBBB"),
        "message should name both serials: {message}"
    );
}

#[test]
fn target_model_mismatch_invalidates_checkpoint() {
    let dir = TempDir::new().unwrap();
    let size = 4 * 1024 * 1024 * 1024;
    let (checkpoint, info) = checkpoint_for(&dir, &drive(size, None, Some("Cruzer Blade")));

    let validation = validate_checkpoint(&checkpoint, &info, &drive(size, None, Some("Ultra Fit")));

    assert!(
        !validation.valid,
        "a drive of another model must not be resumed onto"
    );
    let message = validation.messages.join("\n");
    assert!(
        message.contains("Cruzer Blade") && message.contains("Ultra Fit"),
        "message should name both models: {message}"
    );
}

#[test]
fn unknown_serial_resumes_with_a_warning() {
    let dir = TempDir::new().unwrap();
    let size = 4 * 1024 * 1024 * 1024;

    // A checkpoint written by 0.1.x has no serial, and a platform may
    // report none for a drive. Either way the drive cannot be confirmed.
    let cases = [
        (
            drive(size, None, Some("Flash")),
            drive(size, Some("SER-BBBB"), Some("Flash")),
        ),
        (
            drive(size, Some("SER-AAAA"), Some("Flash")),
            drive(size, None, Some("Flash")),
        ),
        (
            drive(size, None, Some("Flash")),
            drive(size, None, Some("Flash")),
        ),
    ];
    for (made_for, now) in cases {
        let (checkpoint, info) = checkpoint_for(&dir, &made_for);

        let validation = validate_checkpoint(&checkpoint, &info, &now);

        assert!(
            validation.valid,
            "an unknown serial must not block resume ({made_for:?} -> {now:?}): {:?}",
            validation.messages
        );
        assert!(
            validation.warnings.iter().any(|w| w.contains("serial")),
            "an unconfirmed drive should produce a serial warning ({made_for:?} -> {now:?}): {:?}",
            validation.warnings
        );
    }
}

#[test]
fn same_drive_resumes_without_a_warning() {
    let dir = TempDir::new().unwrap();
    let size = 4 * 1024 * 1024 * 1024;
    let stick = drive(size, Some("SER-AAAA"), Some("Flash"));
    let (checkpoint, info) = checkpoint_for(&dir, &stick);

    let validation = validate_checkpoint(&checkpoint, &info, &stick);

    assert!(validation.valid, "{:?}", validation.messages);
    assert!(
        validation.warnings.is_empty(),
        "a confirmed drive needs no warning: {:?}",
        validation.warnings
    );
}

#[test]
fn checkpoint_json_without_identity_fields_still_loads() {
    // Checkpoints written by 0.1.x have no target_serial or target_model.
    // They must still load, with both unknown.
    let dir = TempDir::new().unwrap();
    let (checkpoint, _) = checkpoint_for(&dir, &drive(1024, Some("SER-AAAA"), Some("Flash")));

    let mut json = serde_json::to_value(&checkpoint).unwrap();
    let fields = json.as_object_mut().unwrap();
    assert!(fields.remove("target_serial").is_some());
    assert!(fields.remove("target_model").is_some());
    let path = dir.path().join("old.checkpoint");
    std::fs::write(&path, serde_json::to_vec_pretty(&json).unwrap()).unwrap();

    let mgr = CheckpointManager::new(dir.path().join("checkpoints")).unwrap();
    let loaded = mgr.load_from_path(&path).unwrap();

    assert_eq!(loaded.target_serial, None);
    assert_eq!(loaded.target_model, None);
    assert_eq!(loaded.target_size, 1024);
    assert_eq!(loaded.target_path, "/dev/imaginary");
}

// =============================================================================
// 4b. Saving and finding checkpoints must not hide a failure
// =============================================================================

#[cfg(target_os = "linux")]
#[test]
fn save_fails_when_the_checkpoint_cannot_reach_disk() {
    let dir = TempDir::new().unwrap();
    let (checkpoint, _) = checkpoint_for(&dir, &target(1024));
    let mgr = CheckpointManager::new(dir.path().join("checkpoints")).unwrap();

    // Point the temp file at /dev/full: it opens, and every write to it fails
    // with ENOSPC, like a full disk.
    let final_path = mgr.checkpoint_path(&checkpoint);
    std::os::unix::fs::symlink("/dev/full", final_path.with_extension("tmp")).unwrap();

    let result = mgr.save(&checkpoint);

    assert!(
        result.is_err(),
        "save must report that the checkpoint was not written"
    );
    assert!(
        !final_path.exists(),
        "a checkpoint that was not written must not be put in place"
    );
}

#[test]
fn find_checkpoint_keeps_a_checkpoint_it_cannot_load() {
    let dir = TempDir::new().unwrap();
    let (mut checkpoint, _) = checkpoint_for(&dir, &target(1024));
    // As if a newer engraver wrote it.
    checkpoint.version = CHECKPOINT_VERSION + 1;
    let mgr = CheckpointManager::new(dir.path().join("checkpoints")).unwrap();
    mgr.save(&checkpoint).unwrap();
    let path = mgr.checkpoint_path(&checkpoint);
    let before = read_file(&path);

    let found = mgr.find_checkpoint(&checkpoint.source_path, &checkpoint.target_path, None);

    assert!(
        found.is_err(),
        "an unloadable checkpoint should come back as an error, not as no checkpoint"
    );
    assert!(path.exists(), "find_checkpoint must not delete the file");
    assert_eq!(read_file(&path), before, "or change it");
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
    let original = WriteCheckpoint::new(
        &info,
        "/dev/test",
        &target(1024 * 1024),
        &WriteConfig::new(),
    );
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
