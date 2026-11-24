//! Integration tests for checksumming exactly the first `len` bytes of a reader

#![cfg(feature = "checksum")]

use engraver_core::{ChecksumAlgorithm, Error, Verifier, VerifyConfig, MIN_VERIFY_BLOCK_SIZE};
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

// ============================================================================
// Helpers
// ============================================================================

const BLOCK: usize = MIN_VERIFY_BLOCK_SIZE;

fn test_data(size: usize) -> Vec<u8> {
    (0..size).map(|i| (i % 251) as u8).collect()
}

fn image_then_junk(image: &[u8], extra: usize) -> Vec<u8> {
    let mut drive = image.to_vec();
    drive.extend(std::iter::repeat_n(0xEE, extra));
    drive
}

fn verifier() -> Verifier {
    Verifier::with_config(VerifyConfig::new().block_size(BLOCK))
}

/// The checksum of the whole of `data`, the answer a prefix checksum must give
fn checksum_of(data: &[u8], algorithm: ChecksumAlgorithm) -> String {
    verifier()
        .calculate_checksum(&mut Cursor::new(data), algorithm, None)
        .unwrap()
        .to_hex()
}

fn prefix_checksum(drive: &[u8], len: usize, algorithm: ChecksumAlgorithm) -> String {
    verifier()
        .calculate_checksum_of_prefix(&mut Cursor::new(drive), algorithm, len as u64)
        .unwrap()
        .to_hex()
}

// ============================================================================
// The prefix is what is hashed
// ============================================================================

#[test]
fn prefix_checksum_of_image_then_junk_equals_checksum_of_the_image() {
    let image = test_data(3 * BLOCK + 17);
    let drive = image_then_junk(&image, 1024 * 1024);

    for algorithm in ChecksumAlgorithm::all() {
        assert_eq!(
            prefix_checksum(&drive, image.len(), *algorithm),
            checksum_of(&image, *algorithm),
            "{algorithm}"
        );
    }
}

#[test]
fn prefix_checksum_differs_from_the_checksum_of_the_whole_drive() {
    let image = test_data(2 * BLOCK + 5);
    let drive = image_then_junk(&image, BLOCK);

    assert_ne!(
        prefix_checksum(&drive, image.len(), ChecksumAlgorithm::Sha256),
        checksum_of(&drive, ChecksumAlgorithm::Sha256),
    );
}

#[test]
fn prefix_checksum_sees_a_changed_byte_inside_the_image() {
    let image = test_data(3 * BLOCK + 17);
    let mut drive = image_then_junk(&image, BLOCK);
    drive[BLOCK + 1] ^= 0xFF;

    assert_ne!(
        prefix_checksum(&drive, image.len(), ChecksumAlgorithm::Sha256),
        checksum_of(&image, ChecksumAlgorithm::Sha256),
    );
}

#[test]
fn prefix_checksum_ignores_a_changed_byte_after_the_image() {
    let image = test_data(3 * BLOCK + 17);
    let mut drive = image_then_junk(&image, BLOCK);
    let last = drive.len() - 1;
    drive[last] ^= 0xFF;
    drive[image.len()] ^= 0xFF;

    assert_eq!(
        prefix_checksum(&drive, image.len(), ChecksumAlgorithm::Sha256),
        checksum_of(&image, ChecksumAlgorithm::Sha256),
    );
}

// ============================================================================
// Lengths that are not a multiple of the block size
// ============================================================================

#[test]
fn prefix_checksum_works_for_any_length_relative_to_the_block_size() {
    let lengths = [
        0,
        1,
        BLOCK - 1,
        BLOCK,
        BLOCK + 1,
        3 * BLOCK - 1,
        3 * BLOCK,
        3 * BLOCK + 1,
        3 * BLOCK + 17,
    ];
    let drive = test_data(5 * BLOCK);

    for len in lengths {
        assert_eq!(
            prefix_checksum(&drive, len, ChecksumAlgorithm::Sha256),
            checksum_of(&drive[..len], ChecksumAlgorithm::Sha256),
            "len {len}"
        );
    }
}

// ============================================================================
// A reader shorter than the length
// ============================================================================

#[test]
fn prefix_checksum_of_a_reader_shorter_than_len_is_an_error() {
    let cases = [
        (0, 1),                           // empty reader
        (10, 1_000),                      // ends inside the first block
        (2 * BLOCK, 3 * BLOCK),           // ends exactly on a block boundary
        (3 * BLOCK + 16, 3 * BLOCK + 17), // one byte short
    ];

    for (held, len) in cases {
        let drive = test_data(held);

        let err = verifier()
            .calculate_checksum_of_prefix(
                &mut Cursor::new(drive),
                ChecksumAlgorithm::Sha256,
                len as u64,
            )
            .expect_err("a reader shorter than len must not produce a checksum");

        match err {
            Error::SizeMismatch {
                source_size,
                target_size,
            } => {
                assert_eq!(source_size, len as u64, "held {held}, len {len}");
                assert_eq!(target_size, held as u64, "held {held}, len {len}");
            }
            other => panic!("expected SizeMismatch for held {held}, len {len}, got {other:?}"),
        }
    }
}

// ============================================================================
// Cancellation
// ============================================================================

#[test]
fn prefix_checksum_with_a_pending_cancel_returns_cancelled_and_leaves_the_flag_set() {
    let flag = Arc::new(AtomicBool::new(true));
    let mut verifier = Verifier::with_config(VerifyConfig::new().block_size(BLOCK))
        .with_cancel_flag(Arc::clone(&flag));
    let mut reader = Cursor::new(test_data(2 * BLOCK));

    let err = verifier
        .calculate_checksum_of_prefix(&mut reader, ChecksumAlgorithm::Sha256, BLOCK as u64)
        .expect_err("a pending cancel must stop the checksum");

    assert!(matches!(err, Error::Cancelled), "got {err:?}");
    assert!(
        flag.load(Ordering::SeqCst),
        "the verifier must not clear the flag"
    );
    assert_eq!(reader.position(), 0, "nothing may be read after a cancel");
}

#[test]
fn prefix_checksum_cancelled_between_blocks_returns_cancelled() {
    let flag = Arc::new(AtomicBool::new(false));
    let flag_for_callback = Arc::clone(&flag);
    let mut verifier = Verifier::with_config(VerifyConfig::new().block_size(BLOCK))
        .with_cancel_flag(Arc::clone(&flag))
        .on_progress(move |_| flag_for_callback.store(true, Ordering::SeqCst));
    let mut reader = Cursor::new(test_data(4 * BLOCK));

    let err = verifier
        .calculate_checksum_of_prefix(&mut reader, ChecksumAlgorithm::Sha256, 4 * BLOCK as u64)
        .expect_err("a cancel raised mid-run must stop the checksum");

    assert!(matches!(err, Error::Cancelled), "got {err:?}");
    assert!(flag.load(Ordering::SeqCst));
}

// ============================================================================
// Progress
// ============================================================================

#[test]
fn prefix_checksum_progress_ends_at_len_and_never_passes_it() {
    let len = 3 * BLOCK + 17;
    let drive = test_data(5 * BLOCK);
    let last = Arc::new(AtomicU64::new(0));
    let totals = Arc::new(Mutex::new(Vec::new()));
    let (last_cb, totals_cb) = (Arc::clone(&last), Arc::clone(&totals));
    let mut verifier =
        Verifier::with_config(VerifyConfig::new().block_size(BLOCK)).on_progress(move |p| {
            last_cb.store(p.bytes_processed, Ordering::SeqCst);
            totals_cb.lock().unwrap().push(p.total_bytes);
        });

    verifier
        .calculate_checksum_of_prefix(
            &mut Cursor::new(drive),
            ChecksumAlgorithm::Sha256,
            len as u64,
        )
        .unwrap();

    assert_eq!(last.load(Ordering::SeqCst), len as u64);
    assert!(totals
        .lock()
        .unwrap()
        .iter()
        .all(|total| *total == Some(len as u64)));
}
