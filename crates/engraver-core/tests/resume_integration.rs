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
// 4c. Another drive's checkpoint is kept, and found again when it comes back
// =============================================================================

/// A source image on disk, and the SourceInfo that describes it.
fn source_in(dir: &TempDir) -> SourceInfo {
    let source_path = dir.path().join("source.iso");
    let size = 128 * 1024;
    write_file(&source_path, &image_bytes(size, 0xCC));
    local_source_info_with_hash(&source_path, size as u64)
}

/// A checkpoint for `info` written to `target_path` on a drive `made_for`.
fn checkpoint_on(
    info: &SourceInfo,
    target_path: &str,
    made_for: &TargetIdentity,
) -> WriteCheckpoint {
    WriteCheckpoint::new(info, target_path, made_for, &WriteConfig::new())
}

const STICK_SIZE: u64 = 4 * 1024 * 1024 * 1024;

#[test]
fn checkpoint_of_another_drive_is_not_found_at_the_same_path() {
    let dir = TempDir::new().unwrap();
    let info = source_in(&dir);
    let mgr = CheckpointManager::new(dir.path().join("checkpoints")).unwrap();
    let a = drive(STICK_SIZE, Some("SER-AAAA"), Some("Flash"));
    mgr.save(&checkpoint_on(&info, "/dev/sdb", &a)).unwrap();

    // Another stick shows up at /dev/sdb, with a serial or without one. A
    // lookup that only knew the path would hand it A's checkpoint.
    for serial_b in [Some("SER-BBBB"), None] {
        let found = mgr
            .find_checkpoint(&info.path, "/dev/sdb", serial_b)
            .unwrap();

        assert!(
            found.is_none(),
            "a checkpoint of drive SER-AAAA must not be found for {serial_b:?} at the same path"
        );
    }
}

#[test]
fn checkpoint_of_another_drive_survives_a_save_and_remove_at_the_same_path() {
    let dir = TempDir::new().unwrap();
    let info = source_in(&dir);
    let mgr = CheckpointManager::new(dir.path().join("checkpoints")).unwrap();
    let a = checkpoint_on(
        &info,
        "/dev/sdb",
        &drive(STICK_SIZE, Some("SER-AAAA"), Some("Flash")),
    );
    mgr.save(&a).unwrap();
    let a_path = mgr.checkpoint_path(&a);
    let a_before = read_file(&a_path);

    // A fresh write to another stick at the same path saves its own
    // checkpoint when cancelled and removes it when it completes.
    for b_drive in [
        drive(STICK_SIZE, Some("SER-BBBB"), Some("Flash")),
        drive(STICK_SIZE, None, Some("Flash")),
    ] {
        let b = checkpoint_on(&info, "/dev/sdb", &b_drive);
        mgr.save(&b).unwrap();
        assert_eq!(
            read_file(&a_path),
            a_before,
            "saving a checkpoint for {b_drive:?} must not replace drive A's"
        );
        mgr.remove(&b).unwrap();
        assert!(
            a_path.exists(),
            "removing the checkpoint of {b_drive:?} must not remove drive A's"
        );
        assert_eq!(read_file(&a_path), a_before, "or change it");
    }
}

#[test]
fn checkpoint_is_found_when_its_drive_comes_back_at_another_path() {
    let dir = TempDir::new().unwrap();
    let info = source_in(&dir);
    let mgr = CheckpointManager::new(dir.path().join("checkpoints")).unwrap();
    let a = drive(STICK_SIZE, Some("SER-AAAA"), Some("Flash"));
    mgr.save(&checkpoint_on(&info, "/dev/sdb", &a)).unwrap();

    // The stick is plugged in again and the kernel names it /dev/sdc.
    let found = mgr
        .find_checkpoint(&info.path, "/dev/sdc", Some("SER-AAAA"))
        .unwrap();

    let found = found.expect("the checkpoint of drive SER-AAAA should be found at /dev/sdc");
    assert_eq!(found.target_serial.as_deref(), Some("SER-AAAA"));
    assert_eq!(found.target_path, "/dev/sdb");
}

#[test]
fn checkpoint_without_a_serial_is_still_found_for_a_drive_with_one() {
    // Written by 0.1.x, or for a drive that reports no serial: its file is named
    // after the path alone. A drive with a serial looks for its own file first
    // and then falls back to that one, so these checkpoints are not stranded.
    let dir = TempDir::new().unwrap();
    let info = source_in(&dir);
    let mgr = CheckpointManager::new(dir.path().join("checkpoints")).unwrap();
    let legacy = checkpoint_on(&info, "/dev/sdb", &drive(STICK_SIZE, None, Some("Flash")));
    mgr.save(&legacy).unwrap();

    let found = mgr
        .find_checkpoint(&info.path, "/dev/sdb", Some("SER-AAAA"))
        .unwrap();

    let found = found.expect("a checkpoint without a serial should be found through the fallback");
    assert_eq!(found.target_serial, None);
    assert_eq!(found.target_path, "/dev/sdb");
}

#[test]
fn checkpoint_of_the_drive_is_preferred_over_one_without_a_serial() {
    // Both files exist at /dev/sdb. A drive with a serial must get its own, not
    // the one that may belong to any drive that was ever at that path.
    let dir = TempDir::new().unwrap();
    let info = source_in(&dir);
    let mgr = CheckpointManager::new(dir.path().join("checkpoints")).unwrap();
    let mut legacy = checkpoint_on(&info, "/dev/sdb", &drive(STICK_SIZE, None, Some("Flash")));
    legacy.bytes_written = 1024;
    mgr.save(&legacy).unwrap();
    let mut own = checkpoint_on(
        &info,
        "/dev/sdb",
        &drive(STICK_SIZE, Some("SER-AAAA"), Some("Flash")),
    );
    own.bytes_written = 2048;
    mgr.save(&own).unwrap();

    let found = mgr
        .find_checkpoint(&info.path, "/dev/sdb", Some("SER-AAAA"))
        .unwrap()
        .expect("a checkpoint should be found");

    assert_eq!(found.target_serial.as_deref(), Some("SER-AAAA"));
    assert_eq!(found.bytes_written, 2048);
}

#[test]
fn a_drive_mismatch_is_marked_as_belonging_to_another_drive() {
    let dir = TempDir::new().unwrap();
    let made_for = drive(STICK_SIZE, Some("SER-AAAA"), Some("Flash"));
    let (checkpoint, info) = checkpoint_for(&dir, &made_for);

    let cases = [
        (
            "size",
            drive(2 * STICK_SIZE, Some("SER-AAAA"), Some("Flash")),
        ),
        ("serial", drive(STICK_SIZE, Some("SER-BBBB"), Some("Flash"))),
        (
            "model",
            drive(STICK_SIZE, Some("SER-AAAA"), Some("Ultra Fit")),
        ),
    ];
    for (what, now) in cases {
        let validation = validate_checkpoint(&checkpoint, &info, &now);

        assert!(!validation.valid, "a {what} mismatch should be invalid");
        assert!(
            validation.for_another_drive,
            "a {what} mismatch means the checkpoint is for another drive"
        );
    }

    let same = validate_checkpoint(&checkpoint, &info, &made_for);
    assert!(same.valid);
    assert!(
        !same.for_another_drive,
        "a valid checkpoint is not for another drive"
    );
}

#[test]
fn a_source_problem_is_not_marked_as_belonging_to_another_drive() {
    let dir = TempDir::new().unwrap();
    let made_for = drive(STICK_SIZE, Some("SER-AAAA"), Some("Flash"));
    let (checkpoint, info) = checkpoint_for(&dir, &made_for);

    let mut other_path = info.clone();
    other_path.path = "/somewhere/else.iso".to_string();
    let mut other_size = info.clone();
    other_size.size = Some(info.size.unwrap() + 1);
    let mut other_content = info.clone();
    other_content.source_header_hash = Some("0".repeat(64));
    let mut other_etag = info.clone();
    other_etag.etag = Some("\"new\"".to_string());
    let mut checkpoint_with_etag = checkpoint.clone();
    checkpoint_with_etag.etag = Some("\"old\"".to_string());

    // The drive is the right one here and also a wrong one: when the source is
    // the problem, the checkpoint is no good for its own drive either.
    for now in [
        made_for.clone(),
        drive(2 * STICK_SIZE, Some("SER-BBBB"), Some("Other")),
    ] {
        for (what, cp, source) in [
            ("path", &checkpoint, &other_path),
            ("size", &checkpoint, &other_size),
            ("content", &checkpoint, &other_content),
            ("etag", &checkpoint_with_etag, &other_etag),
        ] {
            let validation = validate_checkpoint(cp, source, &now);

            assert!(
                !validation.valid,
                "a changed source {what} should be invalid"
            );
            assert!(
                !validation.for_another_drive,
                "a changed source {what} is not a reason to keep the checkpoint ({now:?})"
            );
        }
    }
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
