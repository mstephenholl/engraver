//! End-to-end device workflow tests
//!
//! These tests validate actual CLI workflows (write, verify, erase) against
//! a real block device. They are **destructive** and require:
//!
//! 1. A removable drive connected to the system
//! 2. Root/admin privileges (`sudo`)
//! 3. The `ENGRAVER_TEST_DEVICE` environment variable set to the device path
//!    e.g., `ENGRAVER_TEST_DEVICE=/dev/sdb` (Linux) or `/dev/disk4` (macOS)
//!
//! All tests are `#[ignore]` by default. Run them explicitly with:
//!
//! ```bash
//! sudo ENGRAVER_TEST_DEVICE=/dev/sdX cargo test -p engraver --test device_tests -- --ignored
//! ```
//!
//! **WARNING**: All data on the test device WILL be destroyed!

use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

/// Size of the test image (1 MiB) — small enough to be fast, large enough
/// to exercise multi-block I/O with the minimum 4K block size.
const TEST_IMAGE_SIZE: usize = 1024 * 1024;

/// Get the test device path from the environment, or None if not set.
fn test_device() -> Option<String> {
    std::env::var("ENGRAVER_TEST_DEVICE").ok()
}

/// Skip helper: returns the device path or panics with a skip-like message.
fn require_test_device() -> String {
    test_device().unwrap_or_else(|| {
        panic!(
            "ENGRAVER_TEST_DEVICE not set. \
             Set it to a removable drive path to run device tests. \
             Example: sudo ENGRAVER_TEST_DEVICE=/dev/sdb cargo test -p engraver --test device_tests -- --ignored"
        )
    })
}

/// Create a deterministic test image with a recognizable byte pattern.
/// Uses `(offset % 251)` — a prime modulus that avoids aligning with
/// power-of-two block boundaries, making corruption easy to detect.
fn create_test_image(dir: &TempDir) -> std::path::PathBuf {
    let path = dir.path().join("test.img");
    let data: Vec<u8> = (0..TEST_IMAGE_SIZE).map(|i| (i % 251) as u8).collect();
    fs::write(&path, &data).expect("Failed to create test image");
    path
}

/// Create a test image filled entirely with a single byte value.
fn create_filled_image(dir: &TempDir, name: &str, byte: u8, size: usize) -> std::path::PathBuf {
    let path = dir.path().join(name);
    let data = vec![byte; size];
    fs::write(&path, &data).expect("Failed to create filled image");
    path
}

/// Get a command for the engraver binary
#[allow(deprecated)]
fn engraver() -> Command {
    Command::cargo_bin("engraver").unwrap()
}

// ============================================================================
// Write workflow tests
// ============================================================================

#[test]
#[ignore]
fn test_write_image_to_device() {
    let device = require_test_device();
    let dir = TempDir::new().unwrap();
    let image = create_test_image(&dir);

    engraver()
        .args([
            "--silent",
            "write",
            image.to_str().unwrap(),
            &device,
            "--yes",
            "--force",
            "--no-unmount",
            "--block-size",
            "4K",
        ])
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_write_and_verify() {
    let device = require_test_device();
    let dir = TempDir::new().unwrap();
    let image = create_test_image(&dir);

    // Write with inline verification (--verify flag)
    engraver()
        .args([
            "--silent",
            "write",
            image.to_str().unwrap(),
            &device,
            "--yes",
            "--force",
            "--no-unmount",
            "--verify",
            "--block-size",
            "4K",
        ])
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_write_then_standalone_verify() {
    let device = require_test_device();
    let dir = TempDir::new().unwrap();
    let image = create_test_image(&dir);

    // Step 1: Write without inline verify
    engraver()
        .args([
            "--silent",
            "write",
            image.to_str().unwrap(),
            &device,
            "--yes",
            "--force",
            "--no-unmount",
            "--block-size",
            "4K",
        ])
        .assert()
        .success();

    // Step 2: Verify separately using the verify subcommand
    engraver()
        .args([
            "--silent",
            "verify",
            image.to_str().unwrap(),
            &device,
            "--block-size",
            "4K",
        ])
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_write_with_different_block_sizes() {
    let device = require_test_device();
    let dir = TempDir::new().unwrap();
    let image = create_test_image(&dir);

    for block_size in &["4K", "64K", "1M"] {
        // Write with the given block size and verify
        engraver()
            .args([
                "--silent",
                "write",
                image.to_str().unwrap(),
                &device,
                "--yes",
                "--force",
                "--no-unmount",
                "--verify",
                "--block-size",
                block_size,
            ])
            .assert()
            .success();
    }
}

// ============================================================================
// Erase workflow tests
// ============================================================================

#[test]
#[ignore]
fn test_erase_device() {
    let device = require_test_device();

    engraver()
        .args([
            "--silent",
            "erase",
            &device,
            "--yes",
            "--force",
            "--no-unmount",
            "--block-size",
            "4K",
        ])
        .assert()
        .success();
}

#[test]
#[ignore]
fn test_write_then_erase_then_verify_zeros() {
    let device = require_test_device();
    let dir = TempDir::new().unwrap();
    let image = create_test_image(&dir);

    // Step 1: Write non-zero data
    engraver()
        .args([
            "--silent",
            "write",
            image.to_str().unwrap(),
            &device,
            "--yes",
            "--force",
            "--no-unmount",
            "--block-size",
            "4K",
        ])
        .assert()
        .success();

    // Step 2: Erase the device (zero-fill)
    engraver()
        .args([
            "--silent",
            "erase",
            &device,
            "--yes",
            "--force",
            "--no-unmount",
            "--block-size",
            "4K",
        ])
        .assert()
        .success();

    // Step 3: Verify that the region we wrote is now zeros.
    // Write a zero-filled image of the same size and verify it matches.
    let zero_image = create_filled_image(&dir, "zeros.img", 0u8, TEST_IMAGE_SIZE);

    engraver()
        .args([
            "--silent",
            "verify",
            zero_image.to_str().unwrap(),
            &device,
            "--block-size",
            "4K",
        ])
        .assert()
        .success();
}

// ============================================================================
// Verify detects corruption
// ============================================================================

#[test]
#[ignore]
fn test_verify_detects_mismatch() {
    let device = require_test_device();
    let dir = TempDir::new().unwrap();
    let image = create_test_image(&dir);

    // Write the test image
    engraver()
        .args([
            "--silent",
            "write",
            image.to_str().unwrap(),
            &device,
            "--yes",
            "--force",
            "--no-unmount",
            "--block-size",
            "4K",
        ])
        .assert()
        .success();

    // Verify against a *different* image — should fail
    let wrong_image = create_filled_image(&dir, "wrong.img", 0xFF, TEST_IMAGE_SIZE);

    engraver()
        .args([
            "--silent",
            "verify",
            wrong_image.to_str().unwrap(),
            &device,
            "--block-size",
            "4K",
        ])
        .assert()
        .failure();
}

// ============================================================================
// Safety checks (these do NOT require a real device)
// ============================================================================
// Note: write rejection tests (nonexistent device, missing source) are already
// covered by cli_tests.rs (test_write_invalid_target, test_write_missing_source).
// Only erase rejection is tested here since cli_tests.rs has no erase coverage.

#[test]
fn test_erase_rejects_nonexistent_device() {
    engraver()
        .args([
            "--silent",
            "erase",
            "/dev/definitely_not_a_real_device_xyz",
            "--yes",
        ])
        .assert()
        .failure();
}

// ============================================================================
// Full lifecycle: write → verify → erase → verify zeros
// ============================================================================

#[test]
#[ignore]
fn test_full_device_lifecycle() {
    let device = require_test_device();
    let dir = TempDir::new().unwrap();
    let image = create_test_image(&dir);
    let zero_image = create_filled_image(&dir, "zeros.img", 0u8, TEST_IMAGE_SIZE);

    // 1. Write image to device
    engraver()
        .args([
            "--silent",
            "write",
            image.to_str().unwrap(),
            &device,
            "--yes",
            "--force",
            "--no-unmount",
            "--block-size",
            "64K",
        ])
        .assert()
        .success();

    // 2. Verify image was written correctly
    engraver()
        .args([
            "--silent",
            "verify",
            image.to_str().unwrap(),
            &device,
            "--block-size",
            "64K",
        ])
        .assert()
        .success();

    // 3. Erase the device
    engraver()
        .args([
            "--silent",
            "erase",
            &device,
            "--yes",
            "--force",
            "--no-unmount",
            "--block-size",
            "64K",
        ])
        .assert()
        .success();

    // 4. Verify erase: device should now match all-zeros image
    engraver()
        .args([
            "--silent",
            "verify",
            zero_image.to_str().unwrap(),
            &device,
            "--block-size",
            "64K",
        ])
        .assert()
        .success();
}

// ============================================================================
// Checkpoint and resume workflow
// ============================================================================

#[test]
#[ignore]
fn test_write_with_checkpoint() {
    let device = require_test_device();
    let dir = TempDir::new().unwrap();
    let image = create_test_image(&dir);

    // Write with checkpoint enabled — should succeed and create checkpoint data
    engraver()
        .args([
            "--silent",
            "write",
            image.to_str().unwrap(),
            &device,
            "--yes",
            "--force",
            "--no-unmount",
            "--checkpoint",
            "--block-size",
            "4K",
        ])
        .assert()
        .success();

    // Verify the write was correct
    engraver()
        .args([
            "--silent",
            "verify",
            image.to_str().unwrap(),
            &device,
            "--block-size",
            "4K",
        ])
        .assert()
        .success();
}
