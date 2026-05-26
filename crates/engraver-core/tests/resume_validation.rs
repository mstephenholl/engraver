//! Integration tests for content-aware resume validation.
//!
//! These tests exercise the public `validate_checkpoint` API directly,
//! without involving any block device or network. They guard against the
//! regression where a checkpoint validates against a source that has the
//! same path and size but different bytes — silent corruption on resume.

use engraver_core::resume::{validate_checkpoint, WriteCheckpoint};
use engraver_core::source::{SourceInfo, SourceType};
use engraver_core::WriteConfig;

fn local_source(path: &str, size: u64) -> SourceInfo {
    SourceInfo {
        path: path.to_string(),
        source_type: SourceType::LocalFile,
        compressed_size: Some(size),
        size: Some(size),
        seekable: true,
        resumable: false,
        content_type: None,
        etag: None,
        source_header_hash: None,
    }
}

fn http_source(url: &str, size: u64) -> SourceInfo {
    SourceInfo {
        path: url.to_string(),
        source_type: SourceType::Remote,
        compressed_size: Some(size),
        size: Some(size),
        seekable: false,
        resumable: true,
        content_type: None,
        etag: None,
        source_header_hash: None,
    }
}

#[test]
fn resume_rejects_replaced_local_image_with_same_size() {
    // User starts writing image_A.iso (1 GB).
    let mut original = local_source("/tmp/image.iso", 1024 * 1024 * 1024);
    original.source_header_hash = Some("hash_of_image_A".to_string());

    let cp = WriteCheckpoint::new(
        &original,
        "/dev/sdb",
        4 * 1024 * 1024 * 1024,
        &WriteConfig::new(),
    );
    assert_eq!(cp.source_header_hash.as_deref(), Some("hash_of_image_A"));

    // User replaces /tmp/image.iso with image_B.iso of the same size, then
    // tries to resume. The caller is expected to have recomputed the hash
    // of the file currently at that path.
    let mut replaced = local_source("/tmp/image.iso", 1024 * 1024 * 1024);
    replaced.source_header_hash = Some("hash_of_image_B".to_string());

    let result = validate_checkpoint(&cp, &replaced, 4 * 1024 * 1024 * 1024);
    assert!(
        !result.valid,
        "replacement must be rejected; got valid result: {:?}",
        result.messages
    );
}

#[test]
fn resume_rejects_http_source_with_changed_etag() {
    // Original HTTP source served etag "abc".
    let mut v1 = http_source("https://example.com/image.iso", 1024 * 1024);
    v1.etag = Some("\"abc\"".to_string());

    let cp = WriteCheckpoint::new(&v1, "/dev/sdb", 2 * 1024 * 1024, &WriteConfig::new());
    assert_eq!(cp.etag.as_deref(), Some("\"abc\""));

    // Server has since updated the file. New etag returned by HEAD request.
    let mut v2 = http_source("https://example.com/image.iso", 1024 * 1024);
    v2.etag = Some("\"def\"".to_string());

    let result = validate_checkpoint(&cp, &v2, 2 * 1024 * 1024);
    assert!(!result.valid, "etag change must be rejected");
    assert!(
        result.messages.iter().any(|m| m.contains("etag")),
        "diagnostic should mention etag: {:?}",
        result.messages
    );
}

#[test]
fn resume_accepts_identical_source() {
    let mut info = local_source("/tmp/image.iso", 1024 * 1024 * 1024);
    info.source_header_hash = Some("identical".to_string());

    let cp = WriteCheckpoint::new(
        &info,
        "/dev/sdb",
        4 * 1024 * 1024 * 1024,
        &WriteConfig::new(),
    );

    let result = validate_checkpoint(&cp, &info, 4 * 1024 * 1024 * 1024);
    assert!(result.valid, "identical source must validate");
}

#[test]
fn resume_tolerates_missing_hash_on_older_checkpoint() {
    // Simulate an in-flight checkpoint written by an older version of
    // engraver that had no source_header_hash field.
    let info = local_source("/tmp/image.iso", 1024 * 1024 * 1024);
    let mut cp = WriteCheckpoint::new(
        &info,
        "/dev/sdb",
        4 * 1024 * 1024 * 1024,
        &WriteConfig::new(),
    );
    cp.source_header_hash = None;

    // Current source happens to have a hash now (fresh write would compute one).
    let mut current = info.clone();
    current.source_header_hash = Some("anything".to_string());

    let result = validate_checkpoint(&cp, &current, 4 * 1024 * 1024 * 1024);
    assert!(
        result.valid,
        "missing checkpoint hash must not block resume; old checkpoints stay valid"
    );
}
