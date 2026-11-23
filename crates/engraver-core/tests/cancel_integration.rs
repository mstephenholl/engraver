//! Integration tests for the cancel-flag ownership contract.

use engraver_core::{Error, Verifier, VerifyConfig, WriteConfig, Writer, MIN_VERIFY_BLOCK_SIZE};
use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[cfg(feature = "checksum")]
use engraver_core::ChecksumAlgorithm;

const SIZE: usize = 64 * 1024;
const SENTINEL: u8 = 0x5A;
const IMAGE_BYTE: u8 = 0xAB;

fn pending_cancel() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(true))
}

fn writer_sharing(flag: &Arc<AtomicBool>) -> Writer {
    let config = WriteConfig::new().block_size(4096);
    Writer::with_config(config).with_cancel_flag(Arc::clone(flag))
}

fn verifier_sharing(flag: &Arc<AtomicBool>) -> Verifier {
    let config = VerifyConfig::new().block_size(MIN_VERIFY_BLOCK_SIZE);
    Verifier::with_config(config).with_cancel_flag(Arc::clone(flag))
}

// ============================================================================
// Writer
// ============================================================================

#[test]
fn write_with_pending_cancel_returns_cancelled_and_leaves_target_untouched() {
    let flag = pending_cancel();
    let mut writer = writer_sharing(&flag);
    let source = Cursor::new(vec![IMAGE_BYTE; SIZE]);
    let mut target = Cursor::new(vec![SENTINEL; SIZE]);

    let result = writer.write(source, &mut target, SIZE as u64);

    assert!(
        matches!(result, Err(Error::Cancelled)),
        "a pending cancel must stop the write, got {result:?}"
    );
    assert!(
        target.get_ref().iter().all(|&b| b == SENTINEL),
        "the target must not be modified after a pending cancel"
    );
}

#[test]
fn write_leaves_a_pending_cancel_set_for_the_caller() {
    let flag = pending_cancel();
    let mut writer = writer_sharing(&flag);
    let source = Cursor::new(vec![IMAGE_BYTE; SIZE]);
    let mut target = Cursor::new(vec![SENTINEL; SIZE]);

    let _ = writer.write(source, &mut target, SIZE as u64);

    assert!(
        flag.load(Ordering::SeqCst),
        "the writer must not clear a cancel it did not raise"
    );
}

#[test]
fn write_runs_normally_once_the_caller_clears_the_flag() {
    let flag = pending_cancel();
    let mut writer = writer_sharing(&flag);

    let mut target = Cursor::new(vec![SENTINEL; SIZE]);
    let first = writer.write(
        Cursor::new(vec![IMAGE_BYTE; SIZE]),
        &mut target,
        SIZE as u64,
    );
    assert!(matches!(first, Err(Error::Cancelled)));

    flag.store(false, Ordering::SeqCst);
    let second = writer
        .write(
            Cursor::new(vec![IMAGE_BYTE; SIZE]),
            &mut target,
            SIZE as u64,
        )
        .expect("write must succeed once the caller has cleared the flag");

    assert_eq!(second.bytes_written, SIZE as u64);
    assert!(target.get_ref().iter().all(|&b| b == IMAGE_BYTE));
}

#[cfg(feature = "checksum")]
#[test]
fn write_and_verify_with_pending_cancel_returns_cancelled_and_leaves_target_untouched() {
    let flag = pending_cancel();
    let config = WriteConfig::new()
        .block_size(4096)
        .checksum_algorithm(Some(ChecksumAlgorithm::Sha256));
    let mut writer = Writer::with_config(config).with_cancel_flag(Arc::clone(&flag));
    let source = Cursor::new(vec![IMAGE_BYTE; SIZE]);
    let mut target = Cursor::new(vec![SENTINEL; SIZE]);

    let result = writer.write_and_verify(source, &mut target, SIZE as u64);

    assert!(
        matches!(result, Err(Error::Cancelled)),
        "a pending cancel must stop write_and_verify, got {result:?}"
    );
    assert!(
        target.get_ref().iter().all(|&b| b == SENTINEL),
        "the target must not be modified after a pending cancel"
    );
    assert!(flag.load(Ordering::SeqCst));
}

// ============================================================================
// Verifier
// ============================================================================

#[test]
fn compare_with_pending_cancel_returns_cancelled() {
    let flag = pending_cancel();
    let mut verifier = verifier_sharing(&flag);
    let mut source = Cursor::new(vec![IMAGE_BYTE; SIZE]);
    let mut target = Cursor::new(vec![IMAGE_BYTE; SIZE]);

    let result = verifier.compare(&mut source, &mut target, SIZE as u64);

    assert!(
        matches!(result, Err(Error::Cancelled)),
        "a pending cancel must stop compare, got {result:?}"
    );
    assert!(
        flag.load(Ordering::SeqCst),
        "compare must not clear a cancel it did not raise"
    );
}

#[cfg(feature = "checksum")]
#[test]
fn calculate_checksum_with_pending_cancel_returns_cancelled() {
    let flag = pending_cancel();
    let mut verifier = verifier_sharing(&flag);
    let mut reader = Cursor::new(vec![IMAGE_BYTE; SIZE]);

    let result =
        verifier.calculate_checksum(&mut reader, ChecksumAlgorithm::Sha256, Some(SIZE as u64));

    assert!(
        matches!(result, Err(Error::Cancelled)),
        "a pending cancel must stop calculate_checksum, got {result:?}"
    );
    assert!(
        flag.load(Ordering::SeqCst),
        "calculate_checksum must not clear a cancel it did not raise"
    );
}

#[cfg(feature = "checksum")]
#[test]
fn verify_checksum_with_pending_cancel_returns_cancelled_not_a_mismatch() {
    let flag = pending_cancel();
    let mut verifier = verifier_sharing(&flag);
    let mut reader = Cursor::new(vec![IMAGE_BYTE; SIZE]);

    let wrong_checksum = "00";
    let result = verifier.verify_checksum(
        &mut reader,
        ChecksumAlgorithm::Sha256,
        wrong_checksum,
        Some(SIZE as u64),
    );

    assert!(
        matches!(result, Err(Error::Cancelled)),
        "a pending cancel must stop verify_checksum, got {result:?}"
    );
}

#[test]
fn compare_runs_normally_once_the_caller_clears_the_flag() {
    let flag = pending_cancel();
    let mut verifier = verifier_sharing(&flag);
    let mut source = Cursor::new(vec![IMAGE_BYTE; SIZE]);
    let mut target = Cursor::new(vec![IMAGE_BYTE; SIZE]);

    assert!(matches!(
        verifier.compare(&mut source, &mut target, SIZE as u64),
        Err(Error::Cancelled)
    ));

    flag.store(false, Ordering::SeqCst);
    let result = verifier
        .compare(&mut source, &mut target, SIZE as u64)
        .expect("compare must succeed once the caller has cleared the flag");

    assert!(result.success);
    assert_eq!(result.bytes_verified, SIZE as u64);
}
