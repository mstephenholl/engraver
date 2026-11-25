//! Integration tests for the Engraver CLI
//!
//! These tests verify the CLI behavior without requiring root privileges
//! or actual hardware devices.

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Get a command for the engraver binary
#[allow(deprecated)]
fn engraver() -> Command {
    Command::cargo_bin("engraver").unwrap()
}

// ============================================================================
// Help and Version Tests
// ============================================================================

#[test]
fn test_help_flag() {
    engraver()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("bootable USB drives"))
        .stdout(predicate::str::contains("Commands:"))
        .stdout(predicate::str::contains("write"))
        .stdout(predicate::str::contains("verify"))
        .stdout(predicate::str::contains("list"))
        .stdout(predicate::str::contains("checksum"))
        .stdout(predicate::str::contains("benchmark"));
}

#[test]
fn test_version_flag() {
    engraver()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("engraver"))
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn test_no_args_shows_help() {
    engraver()
        .assert()
        .failure()
        .stderr(predicate::str::contains("Usage:"));
}

// ============================================================================
// Subcommand Help Tests
// ============================================================================

#[test]
fn test_write_help() {
    engraver()
        .args(["write", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Write an image"))
        .stdout(predicate::str::contains("<SOURCE>"))
        .stdout(predicate::str::contains("<TARGET>"))
        .stdout(predicate::str::contains("--verify"));
}

#[test]
fn test_verify_help() {
    engraver()
        .args(["verify", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Verify"))
        .stdout(predicate::str::contains("<SOURCE>"))
        .stdout(predicate::str::contains("<TARGET>"));
}

#[test]
fn test_list_help() {
    engraver()
        .args(["list", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("List"))
        .stdout(predicate::str::contains("--all"))
        .stdout(predicate::str::contains("--json"));
}

#[test]
fn test_checksum_help() {
    engraver()
        .args(["checksum", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("checksum"))
        .stdout(predicate::str::contains("<SOURCE>"))
        .stdout(predicate::str::contains("--algorithm"));
}

// ============================================================================
// List Command Tests
// ============================================================================

#[test]
fn test_list_basic() {
    // List command should work without root (just won't show much)
    engraver().arg("list").assert().success();
}

#[test]
fn test_list_all() {
    engraver().args(["list", "--all"]).assert().success();
}

#[test]
fn test_list_json() {
    engraver()
        .args(["list", "--json"])
        .assert()
        .success()
        // JSON output should be valid (starts with [ or {)
        .stdout(predicate::str::starts_with("[").or(predicate::str::starts_with("{")));
}

#[test]
fn test_list_json_all() {
    engraver()
        .args(["list", "--json", "--all"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("[").or(predicate::str::starts_with("{")));
}

// ============================================================================
// Checksum Command Tests
// ============================================================================

#[test]
fn test_checksum_sha256() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.bin");

    // Create a file with known content
    // SHA-256 of "Hello, World!\n" is known
    fs::write(&test_file, "Hello, World!\n").unwrap();

    engraver()
        .args(["checksum", test_file.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("SHA-256"))
        // SHA-256 of "Hello, World!\n"
        .stdout(predicate::str::contains(
            "c98c24b677eff44860afea6f493bbaec5bb1c4cbb209c6fc2bbb47f66ff2ad31",
        ));
}

#[test]
fn test_checksum_md5() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.bin");

    fs::write(&test_file, "Hello, World!\n").unwrap();

    engraver()
        .args([
            "checksum",
            test_file.to_str().unwrap(),
            "--algorithm",
            "md5",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("MD5"))
        // MD5 of "Hello, World!\n"
        .stdout(predicate::str::contains("bea8252ff4e80f41719ea13cdf007273"));
}

#[test]
fn test_checksum_sha512() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.bin");

    fs::write(&test_file, "Hello, World!\n").unwrap();

    engraver()
        .args([
            "checksum",
            test_file.to_str().unwrap(),
            "--algorithm",
            "sha512",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("SHA-512"));
}

#[test]
fn test_checksum_crc32() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.bin");

    fs::write(&test_file, "Hello, World!\n").unwrap();

    engraver()
        .args([
            "checksum",
            test_file.to_str().unwrap(),
            "--algorithm",
            "crc32",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("CRC32"));
}

#[test]
fn test_checksum_missing_file() {
    engraver()
        .args(["checksum", "/nonexistent/file.iso"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("Failed to validate source")
                .or(predicate::str::contains("not found"))
                .or(predicate::str::contains("No such file")),
        );
}

#[test]
fn test_checksum_invalid_algorithm() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.bin");
    fs::write(&test_file, "test").unwrap();

    engraver()
        .args([
            "checksum",
            test_file.to_str().unwrap(),
            "--algorithm",
            "invalid",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid").or(predicate::str::contains("Invalid")));
}

#[test]
fn test_checksum_empty_file() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("empty.bin");

    fs::write(&test_file, "").unwrap();

    engraver()
        .args(["checksum", test_file.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("SHA-256"))
        // SHA-256 of empty file
        .stdout(predicate::str::contains(
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ));
}

#[test]
fn test_checksum_large_file() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("large.bin");

    // Create a 1MB file
    let data = vec![0xABu8; 1024 * 1024];
    fs::write(&test_file, &data).unwrap();

    engraver()
        .args(["checksum", test_file.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("SHA-256"));
}

// ============================================================================
// Write Command Error Tests
// ============================================================================

#[test]
fn test_write_missing_source() {
    // Note: write command checks for root privileges first,
    // so without root we get a different error
    engraver()
        .args(["write", "/nonexistent/image.iso", "/dev/null"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("not found")
                .or(predicate::str::contains("No such file"))
                .or(predicate::str::contains("privileges required")),
        );
}

#[test]
fn test_write_missing_args() {
    engraver()
        .arg("write")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_write_missing_target() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.iso");
    fs::write(&test_file, "test").unwrap();

    engraver()
        .args(["write", test_file.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_write_invalid_target() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.iso");
    fs::write(&test_file, "test content").unwrap();

    engraver()
        .args([
            "write",
            test_file.to_str().unwrap(),
            "/dev/nonexistent_device",
        ])
        .assert()
        .failure();
}

#[test]
fn test_write_yes_flag() {
    // Test that --yes flag is accepted (even if operation fails)
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.iso");
    fs::write(&test_file, "test").unwrap();

    // This will fail because /dev/nonexistent doesn't exist,
    // but it should accept the --yes flag
    engraver()
        .args([
            "write",
            test_file.to_str().unwrap(),
            "/dev/nonexistent",
            "--yes",
        ])
        .assert()
        .failure();
}

#[test]
fn test_write_verify_flag() {
    // Test that --verify flag is accepted
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.iso");
    fs::write(&test_file, "test").unwrap();

    engraver()
        .args([
            "write",
            test_file.to_str().unwrap(),
            "/dev/nonexistent",
            "--verify",
        ])
        .assert()
        .failure();
}

#[test]
fn test_write_block_size_flag() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.iso");
    fs::write(&test_file, "test").unwrap();

    engraver()
        .args([
            "write",
            test_file.to_str().unwrap(),
            "/dev/nonexistent",
            "--block-size",
            "1048576",
        ])
        .assert()
        .failure();
}

#[test]
fn test_write_auto_checksum_flag() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.iso");
    fs::write(&test_file, "test content").unwrap();

    // --auto-checksum flag should be accepted (will fail for other reasons)
    engraver()
        .args([
            "write",
            test_file.to_str().unwrap(),
            "/dev/nonexistent",
            "--auto-checksum",
        ])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("privileges required")
                .or(predicate::str::contains("Administrator"))
                .or(predicate::str::contains("not found")),
        );
}

#[test]
fn test_write_auto_checksum_from_config() {
    let (temp_dir, config_file) = setup_config_test();

    // Create a config with auto_detect enabled
    fs::write(
        &config_file,
        r#"
[checksum]
auto_detect = true
"#,
    )
    .unwrap();

    let test_file = temp_dir.path().join("test.iso");
    fs::write(&test_file, "test content").unwrap();

    // Command should run (will fail for privilege/device reasons, not config)
    engraver()
        .args([
            "--config-file",
            config_file.to_str().unwrap(),
            "write",
            test_file.to_str().unwrap(),
            "/dev/nonexistent",
        ])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("privileges required")
                .or(predicate::str::contains("Administrator"))
                .or(predicate::str::contains("not found")),
        );
}

// ============================================================================
// Show Partitions Flag Tests
// ============================================================================

#[test]
fn test_write_help_shows_partitions_flag() {
    // The --show-partitions/-p flag should appear in write help
    engraver()
        .args(["write", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--show-partitions").or(predicate::str::contains("-p")));
}

#[test]
fn test_write_show_partitions_flag_accepted() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.iso");
    fs::write(&test_file, "test content").unwrap();

    // --show-partitions flag should be accepted (will fail for privilege reasons)
    engraver()
        .args([
            "write",
            test_file.to_str().unwrap(),
            "/dev/nonexistent",
            "--show-partitions",
        ])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("privileges required")
                .or(predicate::str::contains("Administrator"))
                .or(predicate::str::contains("not found")),
        );
}

#[test]
fn test_write_show_partitions_short_flag_accepted() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.img");
    fs::write(&test_file, "test content").unwrap();

    // -p short flag should be accepted (will fail for privilege reasons)
    engraver()
        .args([
            "write",
            test_file.to_str().unwrap(),
            "/dev/nonexistent",
            "-p",
        ])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("privileges required")
                .or(predicate::str::contains("Administrator"))
                .or(predicate::str::contains("not found")),
        );
}

#[test]
fn test_write_show_partitions_combined_with_verify() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.iso");
    fs::write(&test_file, "test content").unwrap();

    // --show-partitions can be combined with --verify
    engraver()
        .args([
            "write",
            test_file.to_str().unwrap(),
            "/dev/nonexistent",
            "--show-partitions",
            "--verify",
        ])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("privileges required")
                .or(predicate::str::contains("Administrator"))
                .or(predicate::str::contains("not found")),
        );
}

// ============================================================================
// Verify Command Error Tests
// ============================================================================

#[test]
fn test_verify_missing_source() {
    // Note: verify command checks for root privileges first,
    // so without root we get a different error
    engraver()
        .args(["verify", "/nonexistent/image.iso", "/dev/null"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("not found")
                .or(predicate::str::contains("No such file"))
                .or(predicate::str::contains("privileges required")),
        );
}

#[test]
fn test_verify_missing_args() {
    engraver()
        .arg("verify")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_verify_missing_target() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.iso");
    fs::write(&test_file, "test").unwrap();

    engraver()
        .args(["verify", test_file.to_str().unwrap()])
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

// ============================================================================
// Verify Against A Larger Target
// ============================================================================
//
// Each target is a regular file holding the image and then other data; `verify`
// skips its root check for a regular file.

const TRAILING_BYTES: usize = 1024 * 1024;

/// An image whose length is not a multiple of any block size `verify` accepts
fn verify_test_image() -> Vec<u8> {
    (0..1024 * 1024 + 4321)
        .map(|i: usize| (i * 31 % 251) as u8)
        .collect()
}

fn write_gzip(dir: &TempDir, name: &str, image: &[u8]) -> std::path::PathBuf {
    use flate2::{write::GzEncoder, Compression};
    use std::io::Write;

    let path = dir.path().join(name);
    let mut encoder = GzEncoder::new(fs::File::create(&path).unwrap(), Compression::default());
    encoder.write_all(image).unwrap();
    encoder.finish().unwrap();
    path
}

fn write_target(dir: &TempDir, contents: &[u8]) -> std::path::PathBuf {
    let path = dir.path().join("drive.bin");
    fs::write(&path, contents).unwrap();
    path
}

fn image_then_other_data(image: &[u8]) -> Vec<u8> {
    let mut drive = image.to_vec();
    drive.extend(std::iter::repeat_n(0xEE, TRAILING_BYTES));
    drive
}

#[test]
fn test_verify_plain_image_passes_when_the_drive_holds_it_followed_by_other_data() {
    let dir = TempDir::new().unwrap();
    let image = verify_test_image();
    let source = dir.path().join("img.raw");
    fs::write(&source, &image).unwrap();
    let target = write_target(&dir, &image_then_other_data(&image));

    engraver()
        .args(["verify", source.to_str().unwrap(), target.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("passed"));
}

#[test]
fn test_verify_compressed_image_passes_when_the_drive_holds_it_followed_by_other_data() {
    let dir = TempDir::new().unwrap();
    let image = verify_test_image();
    let source = write_gzip(&dir, "img.img.gz", &image);
    let target = write_target(&dir, &image_then_other_data(&image));

    for block_size in ["4K", "64K", "4M"] {
        engraver()
            .args([
                "verify",
                source.to_str().unwrap(),
                target.to_str().unwrap(),
                "--block-size",
                block_size,
            ])
            .assert()
            .success()
            .stdout(predicate::str::contains("passed"));
    }
}

#[test]
fn test_verify_compressed_image_passes_when_the_drive_holds_exactly_the_image() {
    let dir = TempDir::new().unwrap();
    let image = verify_test_image();
    let source = write_gzip(&dir, "img.img.gz", &image);
    let target = write_target(&dir, &image);

    engraver()
        .args(["verify", source.to_str().unwrap(), target.to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn test_verify_compressed_image_fails_when_a_byte_in_the_image_is_wrong() {
    let dir = TempDir::new().unwrap();
    let image = verify_test_image();
    let source = write_gzip(&dir, "img.img.gz", &image);
    let mut drive = image_then_other_data(&image);
    drive[image.len() / 2] ^= 0xFF;
    let target = write_target(&dir, &drive);

    engraver()
        .args(["verify", source.to_str().unwrap(), target.to_str().unwrap()])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("do not match"));
}

#[test]
fn test_verify_compressed_image_fails_when_the_drive_is_shorter_than_the_image() {
    let dir = TempDir::new().unwrap();
    let image = verify_test_image();
    let source = write_gzip(&dir, "img.img.gz", &image);
    let target = write_target(&dir, &image[..image.len() - 1]);

    engraver()
        .args(["verify", source.to_str().unwrap(), target.to_str().unwrap()])
        .assert()
        .failure()
        .code(1)
        .stderr(predicate::str::contains("shorter"));
}

// ============================================================================
// Block Size Validation Tests
// ============================================================================
//
// A block size that is not a power of 2 breaks the alignment arithmetic used by
// direct I/O, so write, erase and verify must reject it before doing any work.

#[test]
fn test_write_block_size_not_power_of_2() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.iso");
    fs::write(&test_file, "test").unwrap();

    engraver()
        .args([
            "write",
            test_file.to_str().unwrap(),
            "/dev/nonexistent",
            "--block-size",
            "3M",
            "--yes",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("must be a power of 2"));
}

#[test]
fn test_erase_block_size_not_power_of_2() {
    engraver()
        .args(["erase", "/dev/nonexistent", "--block-size", "3M", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("must be a power of 2"));
}

#[test]
fn test_verify_block_size_not_power_of_2() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.iso");
    fs::write(&test_file, "test").unwrap();

    engraver()
        .args([
            "verify",
            test_file.to_str().unwrap(),
            "/dev/nonexistent",
            "--block-size",
            "3M",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("must be a power of 2"));
}

// ============================================================================
// Invalid Subcommand Tests
// ============================================================================

#[test]
fn test_invalid_subcommand() {
    engraver()
        .arg("invalid_command")
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid").or(predicate::str::contains("unrecognized")));
}

// ============================================================================
// Environment Variable Tests
// ============================================================================

#[test]
fn test_rust_log_env() {
    // Test that RUST_LOG environment variable is respected
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.bin");
    fs::write(&test_file, "test").unwrap();

    engraver()
        .env("RUST_LOG", "debug")
        .args(["checksum", test_file.to_str().unwrap()])
        .assert()
        .success();
}

// ============================================================================
// Output Format Tests
// ============================================================================

#[test]
fn test_checksum_output_format() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.bin");
    fs::write(&test_file, "test data").unwrap();

    let output = engraver()
        .args(["checksum", test_file.to_str().unwrap()])
        .assert()
        .success();

    // Should contain the checksum file format line
    output.stdout(predicate::str::contains("Checksum file format:"));
}

#[test]
fn test_list_output_contains_drive_info() {
    // This test may show different results based on permissions,
    // but the output format should be consistent
    engraver()
        .args(["list", "--all"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("Found")
                .or(predicate::str::contains("No drives"))
                .or(predicate::str::contains("drive")),
        );
}

// ============================================================================
// Checksum Verification Tests (--verify flag if implemented)
// ============================================================================

#[test]
fn test_checksum_correct_for_binary_data() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("binary.bin");

    // Write binary data (all byte values 0-255)
    let data: Vec<u8> = (0u8..=255).collect();
    fs::write(&test_file, &data).unwrap();

    engraver()
        .args(["checksum", test_file.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("SHA-256"))
        // SHA-256 of bytes 0-255
        .stdout(predicate::str::contains(
            "40aff2e9d2d8922e47afd4648e6967497158785fbd1da870e7110266bf944880",
        ));
}

// ============================================================================
// Concurrent/Stress Tests
// ============================================================================

#[test]
fn test_multiple_checksum_operations() {
    let temp_dir = TempDir::new().unwrap();

    // Create multiple files and checksum them
    for i in 0..5 {
        let test_file = temp_dir.path().join(format!("test{}.bin", i));
        fs::write(&test_file, format!("content {}", i)).unwrap();

        engraver()
            .args(["checksum", test_file.to_str().unwrap()])
            .assert()
            .success();
    }
}

// ============================================================================
// Path Handling Tests
// ============================================================================

#[test]
fn test_checksum_relative_path() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.bin");
    fs::write(&test_file, "test").unwrap();

    // Use the full path (relative paths are tricky in tests)
    engraver()
        .args(["checksum", test_file.to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn test_checksum_path_with_spaces() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test file with spaces.bin");
    fs::write(&test_file, "test").unwrap();

    engraver()
        .args(["checksum", test_file.to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn test_checksum_path_with_unicode() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("тест_файл_🎉.bin");
    fs::write(&test_file, "test").unwrap();

    engraver()
        .args(["checksum", test_file.to_str().unwrap()])
        .assert()
        .success();
}

// ============================================================================
// Exit Code Tests
// ============================================================================

#[test]
fn test_success_exit_code() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.bin");
    fs::write(&test_file, "test").unwrap();

    engraver()
        .args(["checksum", test_file.to_str().unwrap()])
        .assert()
        .code(0);
}

#[test]
fn test_error_exit_code() {
    engraver()
        .args(["checksum", "/nonexistent/file"])
        .assert()
        .code(predicate::ne(0));
}

#[test]
fn test_invalid_args_exit_code() {
    engraver().arg("write").assert().failure(); // Just check it fails, exit code varies
}

#[test]
fn test_runtime_error_exit_code_is_1() {
    engraver()
        .args(["checksum", "/nonexistent/file"])
        .assert()
        .code(1);
}

#[test]
fn test_usage_error_exit_code_is_2() {
    engraver().arg("write").assert().code(2);
}

/// A Ctrl+C that cancels a run must not look like success. Scripts that chain
/// `engraver ... && next-step` rely on a non-zero exit, and 130 is the shell
/// convention for "terminated by SIGINT".
#[cfg(unix)]
#[test]
fn test_checksum_interrupted_by_sigint_exits_130() {
    use std::io::{BufRead, BufReader, Read};
    use std::process::{Command, Stdio};

    let temp_dir = TempDir::new().unwrap();
    let image = temp_dir.path().join("big.img");
    // Sparse, so it costs no disk, but hashing 8 GiB takes far longer than
    // the signal needs to arrive.
    fs::File::create(&image)
        .unwrap()
        .set_len(8 * 1024 * 1024 * 1024)
        .unwrap();

    let mut child = Command::new(env!("CARGO_BIN_EXE_engraver"))
        .args(["checksum", image.to_str().unwrap()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    // The Ctrl+C handler is installed before the command prints anything, so
    // once this line appears the signal reaches the handler, not the default.
    let mut stdout = BufReader::new(child.stdout.take().unwrap());
    let mut line = String::new();
    loop {
        line.clear();
        let read = stdout.read_line(&mut line).unwrap();
        assert!(read > 0, "engraver exited before it started hashing");
        if line.contains("Calculating") {
            break;
        }
    }

    let killed = Command::new("kill")
        .args(["-INT", &child.id().to_string()])
        .status()
        .unwrap();
    assert!(killed.success(), "could not send SIGINT");

    let status = child.wait().unwrap();
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();

    assert_eq!(status.code(), Some(130), "stderr: {stderr}");
    assert!(
        !stderr.contains("Error:"),
        "a cancel is not an error report: {stderr}"
    );
}

// ============================================================================
// Shell Completions Tests
// ============================================================================

#[test]
fn test_completions_help() {
    engraver()
        .args(["completions", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("shell"))
        .stdout(predicate::str::contains("completions"));
}

#[test]
fn test_completions_bash() {
    engraver()
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("_engraver"))
        .stdout(predicate::str::contains("complete"));
}

#[test]
fn test_completions_zsh() {
    engraver()
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("#compdef engraver"));
}

#[test]
fn test_completions_fish() {
    engraver()
        .args(["completions", "fish"])
        .assert()
        .success()
        .stdout(predicate::str::contains("complete"))
        .stdout(predicate::str::contains("engraver"));
}

#[test]
fn test_completions_powershell() {
    engraver()
        .args(["completions", "powershell"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Register-ArgumentCompleter"));
}

#[test]
fn test_completions_elvish() {
    engraver()
        .args(["completions", "elvish"])
        .assert()
        .success()
        .stdout(predicate::str::contains("set edit:completion"));
}

// ============================================================================
// Man Page Tests
// ============================================================================

#[test]
fn test_mangen_help() {
    engraver()
        .args(["mangen", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("man"))
        .stdout(predicate::str::contains("out-dir"));
}

#[test]
fn test_mangen_generates_files() {
    let temp_dir = TempDir::new().unwrap();

    engraver()
        .args(["mangen", "--out-dir", temp_dir.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("engraver.1"));

    // Check that files were created
    assert!(temp_dir.path().join("engraver.1").exists());
    assert!(temp_dir.path().join("engraver-write.1").exists());
    assert!(temp_dir.path().join("engraver-verify.1").exists());
    assert!(temp_dir.path().join("engraver-list.1").exists());
    assert!(temp_dir.path().join("engraver-checksum.1").exists());
}

#[test]
fn test_mangen_content() {
    let temp_dir = TempDir::new().unwrap();

    engraver()
        .args(["mangen", "--out-dir", temp_dir.path().to_str().unwrap()])
        .assert()
        .success();

    // Check main man page content
    let content = fs::read_to_string(temp_dir.path().join("engraver.1")).unwrap();
    assert!(content.contains(".TH")); // Man page header
    assert!(content.contains("engraver"));
    assert!(content.contains("bootable"));
}

// ============================================================================
// Config Command Tests
// ============================================================================

/// Helper to set up config file for tests in a cross-platform way.
/// Uses --config-file flag instead of platform-specific env vars.
/// Returns (temp_dir, config_file)
fn setup_config_test() -> (TempDir, std::path::PathBuf) {
    let temp_dir = TempDir::new().unwrap();
    let config_file = temp_dir.path().join("engraver_config.toml");
    (temp_dir, config_file)
}

#[test]
fn test_config_help() {
    engraver()
        .args(["config", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("configuration"))
        .stdout(predicate::str::contains("--init"))
        .stdout(predicate::str::contains("--path"))
        .stdout(predicate::str::contains("--json"));
}

#[test]
fn test_config_shows_defaults() {
    // Running config without arguments should show current settings
    engraver()
        .arg("config")
        .assert()
        .success()
        .stdout(predicate::str::contains("[write]"))
        .stdout(predicate::str::contains("[checksum]"))
        .stdout(predicate::str::contains("[behavior]"))
        .stdout(predicate::str::contains("block_size"))
        .stdout(predicate::str::contains("verify"))
        .stdout(predicate::str::contains("algorithm"));
}

#[test]
fn test_config_path_flag() {
    // --path should show the config file path
    engraver()
        .args(["config", "--path"])
        .assert()
        .success()
        .stdout(predicate::str::contains("engraver_config.toml"));
}

#[test]
fn test_config_json_output() {
    // --json should output valid JSON
    engraver()
        .args(["config", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::starts_with("{"))
        .stdout(predicate::str::contains("\"write\""))
        .stdout(predicate::str::contains("\"checksum\""))
        .stdout(predicate::str::contains("\"behavior\""))
        .stdout(predicate::str::contains("\"block_size\""))
        .stdout(predicate::str::contains("\"algorithm\""));
}

#[test]
fn test_config_init_creates_file() {
    let (_temp_dir, config_file) = setup_config_test();

    // config --init should create a config file
    engraver()
        .args([
            "--config-file",
            config_file.to_str().unwrap(),
            "config",
            "--init",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created configuration file"));

    // Verify file was created
    assert!(config_file.exists());

    // Read and verify content
    let content = fs::read_to_string(&config_file).unwrap();
    assert!(content.contains("[write]"));
    assert!(content.contains("[checksum]"));
    assert!(content.contains("[behavior]"));
    assert!(content.contains("block_size"));
}

#[test]
fn test_config_init_warns_if_exists() {
    let (_temp_dir, config_file) = setup_config_test();

    // Create the config file first
    fs::write(&config_file, "[write]\nverify = true").unwrap();

    // config --init should warn that file exists
    engraver()
        .args([
            "--config-file",
            config_file.to_str().unwrap(),
            "config",
            "--init",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("already exists"));

    // Verify original content is preserved
    let content = fs::read_to_string(&config_file).unwrap();
    assert!(content.contains("verify = true"));
}

#[test]
fn test_config_loads_custom_settings() {
    let (_temp_dir, config_file) = setup_config_test();

    // Create a custom config file
    fs::write(
        &config_file,
        r#"
[write]
block_size = "2M"
verify = true

[checksum]
algorithm = "sha512"

[behavior]
skip_confirmation = true
"#,
    )
    .unwrap();

    // config should show the custom settings
    engraver()
        .args(["--config-file", config_file.to_str().unwrap(), "config"])
        .assert()
        .success()
        .stdout(predicate::str::contains("2M"))
        .stdout(predicate::str::contains("sha512"));
}

#[test]
fn test_config_json_with_custom_settings() {
    let (_temp_dir, config_file) = setup_config_test();

    // Create a custom config file
    fs::write(
        &config_file,
        r#"
[write]
verify = true
checkpoint = true

[checksum]
auto_detect = true
"#,
    )
    .unwrap();

    // config --json should reflect custom settings
    engraver()
        .args([
            "--config-file",
            config_file.to_str().unwrap(),
            "config",
            "--json",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"verify\": true"))
        .stdout(predicate::str::contains("\"checkpoint\": true"))
        .stdout(predicate::str::contains("\"auto_detect\": true"));
}

#[test]
fn test_config_silent_mode() {
    // --silent should suppress output
    engraver()
        .args(["--silent", "config"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn test_config_path_silent_mode() {
    // --path with --silent should still output the path
    engraver()
        .args(["--silent", "config", "--path"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn test_config_path_json_prints_the_path_as_json() {
    let (_temp_dir, config_file) = setup_config_test();
    let config_path = config_file.to_str().unwrap();

    let output = engraver()
        .args(["--config-file", config_path, "config", "--path", "--json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        stdout_as_json(&output),
        serde_json::json!({ "path": config_path })
    );
    assert!(!config_file.exists(), "--path must not create the file");
}

#[test]
fn test_config_path_json_names_the_default_path_without_config_file() {
    let output = engraver()
        .args(["config", "--path", "--json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let path = stdout_as_json(&output)["path"]
        .as_str()
        .expect("path should be a string")
        .to_owned();
    assert!(path.ends_with("engraver_config.toml"), "got {path:?}");
}

#[test]
fn test_config_init_json_reports_the_created_file() {
    let (_temp_dir, config_file) = setup_config_test();
    let config_path = config_file.to_str().unwrap();

    let output = engraver()
        .args(["--config-file", config_path, "config", "--init", "--json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        stdout_as_json(&output),
        serde_json::json!({ "path": config_path, "created": true })
    );
    assert!(fs::read_to_string(&config_file)
        .unwrap()
        .contains("[write]"));
}

#[test]
fn test_config_init_json_reports_an_existing_file_left_alone() {
    let (_temp_dir, config_file) = setup_config_test();
    let config_path = config_file.to_str().unwrap();
    fs::write(&config_file, "[write]\nverify = true").unwrap();

    let output = engraver()
        .args(["--config-file", config_path, "config", "--init", "--json"])
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        stdout_as_json(&output),
        serde_json::json!({ "path": config_path, "created": false })
    );
    assert_eq!(
        fs::read_to_string(&config_file).unwrap(),
        "[write]\nverify = true"
    );
}

// ============================================================================
// JSON Output Tests
// ============================================================================
//
// `--json` output is for scripts, so stdout must hold the JSON document and
// nothing else. Log lines belong on stderr.

/// Parse stdout as one JSON document, showing the raw text when it is not one.
fn stdout_as_json(output: &std::process::Output) -> serde_json::Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "stdout is not a JSON document ({e}):\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

#[test]
fn test_list_json_stdout_is_json_with_verbose_logging() {
    // -v turns on debug logging, which must not share stdout with the JSON
    for args in [
        ["-v", "list", "--json"].as_slice(),
        ["-v", "list", "--json", "--all"].as_slice(),
    ] {
        let output = engraver().args(args).output().unwrap();

        assert!(output.status.success(), "{args:?} failed");
        assert!(stdout_as_json(&output).is_array(), "{args:?}");
    }
}

#[test]
fn test_list_json_has_a_serial_key_for_every_drive() {
    // The serial is how a script tells two drives of the same model apart. It is
    // null for a drive that reports none, but the key is always there.
    let output = engraver()
        .args(["list", "--json", "--all"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let json = stdout_as_json(&output);
    let drives = json.as_array().expect("list --json is an array");
    // A machine with no drives has nothing to check.
    for drive in drives {
        let serial = drive.get("serial").unwrap_or_else(|| {
            panic!("a drive in list --json has no serial key: {drive}");
        });
        assert!(serial.is_string() || serial.is_null(), "{drive}");
    }
}

#[test]
fn test_json_stdout_is_json_when_config_file_is_invalid() {
    // A config file that does not parse logs a warning at the default log level
    let (_temp_dir, config_file) = setup_config_test();
    fs::write(&config_file, "this is [not valid toml").unwrap();

    for command in [
        ["list", "--json"].as_slice(),
        ["list", "--json", "--all"].as_slice(),
        ["config", "--json"].as_slice(),
        ["config", "--path", "--json"].as_slice(),
        ["config", "--init", "--json"].as_slice(),
    ] {
        let output = engraver()
            .args(["--config-file", config_file.to_str().unwrap()])
            .args(command)
            .output()
            .unwrap();

        assert!(output.status.success(), "{command:?} failed");
        stdout_as_json(&output);
    }
}

#[test]
fn test_logs_go_to_stderr_without_colour_codes_when_piped() {
    let (_temp_dir, config_file) = setup_config_test();
    fs::write(&config_file, "this is [not valid toml").unwrap();

    let output = engraver()
        .args(["--config-file", config_file.to_str().unwrap()])
        .args(["config", "--json"])
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Failed to parse config file"),
        "the warning should be on stderr, got: {stderr:?}"
    );
    assert!(
        !stderr.contains('\u{1b}'),
        "stderr is not a terminal, so it should have no ANSI colour codes, got: {stderr:?}"
    );
}

const MISTYPED_CONFIG: &str = "[write]\nverify = true\nblock_size = 32\n";

fn assert_reports_unusable_config(
    output: &std::process::Output,
    config_path: &std::path::Path,
    what: &str,
) {
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{what} failed: {stderr}");
    assert_eq!(
        stderr.matches(config_path.to_str().unwrap()).count(),
        1,
        "{what}: stderr should name the config file once, got: {stderr:?}"
    );
    assert!(
        stderr.contains("Using default settings"),
        "{what}: stderr should say the defaults are in use, got: {stderr:?}"
    );
    assert!(
        !stderr.contains('\u{1b}'),
        "{what}: stderr is not a terminal, so it should have no colour codes, got: {stderr:?}"
    );
}

#[test]
fn test_invalid_config_file_is_reported_on_stderr_by_every_command() {
    let (temp_dir, config_file) = setup_config_test();
    fs::write(&config_file, MISTYPED_CONFIG).unwrap();
    let image = temp_dir.path().join("image.bin");
    fs::write(&image, b"hello").unwrap();

    for command in [
        vec!["list"],
        vec!["checksum", image.to_str().unwrap()],
        vec!["config"],
        vec!["config", "--json"],
    ] {
        let output = engraver()
            .args(["--config-file", config_file.to_str().unwrap()])
            .args(&command)
            .output()
            .unwrap();

        assert_reports_unusable_config(&output, &config_file, &format!("{command:?}"));
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(
            stderr.matches("Failed to parse config file").count(),
            1,
            "{command:?}: got {stderr:?}"
        );
        assert!(
            stderr.contains("block_size"),
            "{command:?}: stderr should show what is wrong, got: {stderr:?}"
        );
    }
}

#[test]
fn test_unreadable_config_file_is_reported_on_stderr() {
    let dir_as_config_file = TempDir::new().unwrap();

    let output = engraver()
        .args([
            "--config-file",
            dir_as_config_file.path().to_str().unwrap(),
            "list",
        ])
        .output()
        .unwrap();

    assert_reports_unusable_config(&output, dir_as_config_file.path(), "list");
}

#[cfg(unix)]
#[test]
fn test_invalid_config_at_the_default_path_is_reported_on_stderr() {
    let home = TempDir::new().unwrap();
    let engraver_at_home = || {
        let mut command = engraver();
        command
            .env("HOME", home.path())
            .env("XDG_CONFIG_HOME", home.path().join(".config"));
        command
    };

    let output = engraver_at_home()
        .args(["config", "--path"])
        .output()
        .unwrap();
    let config_file = std::path::PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(config_file.starts_with(home.path()), "got {config_file:?}");
    fs::create_dir_all(config_file.parent().unwrap()).unwrap();
    fs::write(&config_file, MISTYPED_CONFIG).unwrap();

    let output = engraver_at_home().arg("list").output().unwrap();

    assert_reports_unusable_config(&output, &config_file, "list");
}

#[test]
fn test_invalid_config_warning_stays_off_json_stdout() {
    let (_temp_dir, config_file) = setup_config_test();
    fs::write(&config_file, MISTYPED_CONFIG).unwrap();

    let output = engraver()
        .args(["--config-file", config_file.to_str().unwrap()])
        .args(["list", "--json"])
        .output()
        .unwrap();

    assert!(stdout_as_json(&output).is_array());
    assert_reports_unusable_config(&output, &config_file, "list --json");
}

#[test]
fn test_quiet_hides_the_invalid_config_warning() {
    let (_temp_dir, config_file) = setup_config_test();
    fs::write(&config_file, MISTYPED_CONFIG).unwrap();

    engraver()
        .args(["--config-file", config_file.to_str().unwrap()])
        .args(["--quiet", "list"])
        .assert()
        .success()
        .stderr(predicate::str::is_empty());
}

// ============================================================================
// Quiet Output Tests
// ============================================================================
//
// `-q`, and `quiet = true` in the configuration file, hide the command output
// that `--silent` hides: only errors and a command's one useful result remain.
// Unlike `--silent`, they do not skip confirmation prompts.

/// SHA-256 of "Hello, World!\n"
const HELLO_SHA256: &str = "c98c24b677eff44860afea6f493bbaec5bb1c4cbb209c6fc2bbb47f66ff2ad31";

/// A small file holding "Hello, World!\n"
fn hello_file() -> (TempDir, std::path::PathBuf) {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("hello.bin");
    fs::write(&file, "Hello, World!\n").unwrap();
    (dir, file)
}

#[test]
fn test_quiet_checksum_prints_only_the_hash() {
    let (_dir, file) = hello_file();

    for flag in ["-q", "--quiet"] {
        engraver()
            .args([flag, "checksum", file.to_str().unwrap()])
            .assert()
            .success()
            .stdout(format!("{HELLO_SHA256}\n"))
            .stderr(predicate::str::is_empty());
    }
}

#[test]
fn test_config_file_quiet_checksum_prints_only_the_hash() {
    let (dir, file) = hello_file();
    let config_file = dir.path().join("engraver_config.toml");
    fs::write(&config_file, "[behavior]\nquiet = true\n").unwrap();

    engraver()
        .args(["--config-file", config_file.to_str().unwrap()])
        .args(["checksum", file.to_str().unwrap()])
        .assert()
        .success()
        .stdout(format!("{HELLO_SHA256}\n"))
        .stderr(predicate::str::is_empty());
}

#[test]
fn test_config_file_quiet_false_leaves_checksum_output_alone() {
    let (dir, file) = hello_file();
    let config_file = dir.path().join("engraver_config.toml");
    fs::write(&config_file, "[behavior]\nquiet = false\n").unwrap();

    engraver()
        .args(["--config-file", config_file.to_str().unwrap()])
        .args(["checksum", file.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Source:"))
        .stdout(predicate::str::contains("Checksum file format:"));
}

#[test]
fn test_quiet_checksum_still_reports_an_error() {
    let dir = TempDir::new().unwrap();
    let missing = dir.path().join("missing.bin");

    engraver()
        .args(["-q", "checksum", missing.to_str().unwrap()])
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("Failed to validate source"));
}

#[test]
fn test_quiet_list_prints_the_same_drives() {
    let plain = engraver().args(["list", "--all"]).output().unwrap();
    let quiet = engraver().args(["-q", "list", "--all"]).output().unwrap();
    assert!(plain.status.success() && quiet.status.success());
    assert!(!plain.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&quiet.stdout),
        String::from_utf8_lossy(&plain.stdout),
        "-q must not hide the drive list"
    );
    assert!(quiet.stderr.is_empty());

    let output = engraver()
        .args(["-q", "list", "--all", "--json"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(stdout_as_json(&output).is_array());
}

#[test]
fn test_quiet_config_prints_the_same_configuration() {
    let (_temp_dir, config_file) = setup_config_test();
    let config = config_file.to_str().unwrap();

    let plain = engraver()
        .args(["--config-file", config, "config"])
        .output()
        .unwrap();
    let quiet = engraver()
        .args(["--config-file", config, "-q", "config"])
        .output()
        .unwrap();
    assert!(plain.status.success() && quiet.status.success());
    assert!(!plain.stdout.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&quiet.stdout),
        String::from_utf8_lossy(&plain.stdout),
        "-q must not hide the configuration"
    );
}

#[test]
fn test_config_file_quiet_still_prints_the_config_path() {
    // The docs pass `--config-file "$(engraver config --path)"` to sudo, so the
    // path must print for a user who sets `quiet = true`.
    let (_temp_dir, config_file) = setup_config_test();
    fs::write(&config_file, "[behavior]\nquiet = true\n").unwrap();

    engraver()
        .args(["--config-file", config_file.to_str().unwrap()])
        .args(["config", "--path"])
        .assert()
        .success()
        .stdout(format!("{}\n", config_file.display()));
}

#[test]
fn test_quiet_verify_prints_nothing_when_the_drive_matches() {
    let dir = TempDir::new().unwrap();
    let image = verify_test_image();
    let source = dir.path().join("img.raw");
    fs::write(&source, &image).unwrap();
    let target = write_target(&dir, &image_then_other_data(&image));

    engraver()
        .args(["-q", "verify", source.to_str().unwrap()])
        .arg(target.to_str().unwrap())
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::is_empty());
}

#[test]
fn test_quiet_verify_reports_only_the_error_when_the_drive_differs() {
    let dir = TempDir::new().unwrap();
    let image = verify_test_image();
    let source = dir.path().join("img.raw");
    fs::write(&source, &image).unwrap();
    let mut drive = image_then_other_data(&image);
    drive[image.len() / 2] ^= 0xFF;
    let target = write_target(&dir, &drive);

    engraver()
        .args(["-q", "verify", source.to_str().unwrap()])
        .arg(target.to_str().unwrap())
        .assert()
        .failure()
        .code(1)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("Verification failed"));
}

// ============================================================================
// Benchmark Command Tests
// ============================================================================

#[test]
fn test_benchmark_help() {
    engraver()
        .args(["benchmark", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Benchmark"))
        .stdout(predicate::str::contains("<TARGET>"))
        .stdout(predicate::str::contains("--size"))
        .stdout(predicate::str::contains("--block-size"))
        .stdout(predicate::str::contains("--pattern"))
        .stdout(predicate::str::contains("--passes"))
        .stdout(predicate::str::contains("--test-block-sizes"))
        .stdout(predicate::str::contains("--json"))
        .stdout(predicate::str::contains("--yes"));
}

#[test]
fn test_benchmark_missing_target() {
    engraver()
        .arg("benchmark")
        .assert()
        .failure()
        .stderr(predicate::str::contains("required"));
}

#[test]
fn test_benchmark_mutual_exclusivity_size_and_test_block_sizes() {
    // --size and --test-block-sizes are mutually exclusive
    engraver()
        .args([
            "benchmark",
            "/dev/nonexistent",
            "--size",
            "256M",
            "--test-block-sizes",
            "4K,1M,4M",
            "--yes",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "Cannot use both --size and --test-block-sizes",
        ));
}

#[test]
fn test_benchmark_invalid_block_size_not_power_of_2() {
    // Block size must be power of 2
    engraver()
        .args([
            "benchmark",
            "/dev/nonexistent",
            "--block-size",
            "3M",
            "--yes",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("must be a power of 2"));
}

#[test]
fn test_benchmark_invalid_size_not_power_of_2() {
    // Test size must be power of 2
    engraver()
        .args(["benchmark", "/dev/nonexistent", "--size", "100M", "--yes"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("must be a power of 2"));
}

#[test]
fn test_benchmark_size_smaller_than_one_block_is_refused_up_front() {
    engraver()
        .args([
            "benchmark",
            "/dev/nonexistent",
            "--size",
            "4M",
            "--block-size",
            "64M",
            "--yes",
        ])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("smaller than the block size")
                .and(predicate::str::contains("4 MB"))
                .and(predicate::str::contains("64 MB")),
        );
}

const NON_POWER_OF_2_TEST_SIZE_CONFIG: &str = "[benchmark]\ntest_size = \"100M\"\n";

fn benchmark_stderr_with_a_non_power_of_2_configured_test_size(args: &[&str]) -> String {
    let (_temp_dir, config_file) = setup_config_test();
    fs::write(&config_file, NON_POWER_OF_2_TEST_SIZE_CONFIG).unwrap();

    let output = engraver()
        .args(["--config-file", config_file.to_str().unwrap()])
        .args([
            "benchmark",
            "/dev/nonexistent",
            "--block-size",
            "4M",
            "--yes",
        ])
        .args(args)
        .output()
        .unwrap();

    assert!(!output.status.success());
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn test_benchmark_size_ignores_a_configured_test_size_that_is_not_a_power_of_2() {
    let stderr = benchmark_stderr_with_a_non_power_of_2_configured_test_size(&["--size", "32M"]);

    assert!(
        stderr.contains("privileges required") || stderr.contains("Device not found"),
        "the run should get past argument validation, got: {stderr:?}"
    );
}

#[test]
fn test_benchmark_without_size_refuses_a_configured_test_size_that_is_not_a_power_of_2() {
    let stderr = benchmark_stderr_with_a_non_power_of_2_configured_test_size(&[]);

    assert!(stderr.contains("must be a power of 2: 100M"), "{stderr:?}");
}

#[test]
fn test_benchmark_test_block_sizes_refuses_a_configured_test_size_that_is_not_a_power_of_2() {
    let stderr = benchmark_stderr_with_a_non_power_of_2_configured_test_size(&[
        "--test-block-sizes",
        "4K,4M",
    ]);

    assert!(stderr.contains("must be a power of 2: 100M"), "{stderr:?}");
}

#[test]
fn test_benchmark_block_size_too_large() {
    // Block size max is 64M
    engraver()
        .args([
            "benchmark",
            "/dev/nonexistent",
            "--block-size",
            "128M",
            "--yes",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot exceed 64 MB"));
}

#[test]
fn test_benchmark_test_block_sizes_invalid_value() {
    // Each block size in the list must be power of 2
    engraver()
        .args([
            "benchmark",
            "/dev/nonexistent",
            "--test-block-sizes",
            "4K,3M,16M",
            "--yes",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("must be a power of 2"));
}

#[test]
fn test_benchmark_test_block_sizes_exceeds_max() {
    // Each block size in the list must be <= 64M
    engraver()
        .args([
            "benchmark",
            "/dev/nonexistent",
            "--test-block-sizes",
            "4K,1M,128M",
            "--yes",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("exceeds maximum"));
}

#[test]
fn test_benchmark_valid_patterns() {
    // Test that valid patterns are accepted (will fail for other reasons)
    for pattern in ["zeros", "random", "sequential"] {
        engraver()
            .args([
                "benchmark",
                "/dev/nonexistent",
                "--pattern",
                pattern,
                "--yes",
            ])
            .assert()
            .failure()
            // Should fail due to device not found, not due to invalid pattern
            .stderr(
                predicate::str::contains("not found")
                    .or(predicate::str::contains("privileges required"))
                    .or(predicate::str::contains("Administrator"))
                    .or(predicate::str::contains("No such file"))
                    .or(predicate::str::contains("DESTRUCTIVE")),
            );
    }
}

#[test]
fn test_benchmark_invalid_pattern() {
    engraver()
        .args([
            "benchmark",
            "/dev/nonexistent",
            "--pattern",
            "invalid_pattern",
            "--yes",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Unknown pattern"));
}

#[test]
fn test_benchmark_yes_flag_accepted() {
    // --yes flag should be accepted
    engraver()
        .args(["benchmark", "/dev/nonexistent", "--yes"])
        .assert()
        .failure()
        // Should fail for device reasons, not flag parsing
        .stderr(
            predicate::str::contains("not found")
                .or(predicate::str::contains("privileges required"))
                .or(predicate::str::contains("Administrator"))
                .or(predicate::str::contains("No such file"))
                .or(predicate::str::contains("DESTRUCTIVE")),
        );
}

#[test]
fn test_benchmark_json_flag_accepted() {
    // --json flag should be accepted
    engraver()
        .args(["benchmark", "/dev/nonexistent", "--json", "--yes"])
        .assert()
        .failure();
}

#[test]
fn test_benchmark_passes_flag() {
    // --passes flag should accept a number
    engraver()
        .args(["benchmark", "/dev/nonexistent", "--passes", "3", "--yes"])
        .assert()
        .failure();
}

#[test]
fn test_benchmark_size_parsing() {
    // Test various valid size formats
    for size in ["128M", "256M", "512M", "1G"] {
        engraver()
            .args(["benchmark", "/dev/nonexistent", "--size", size, "--yes"])
            .assert()
            .failure()
            // Should fail for device reasons, not size parsing
            .stderr(
                predicate::str::contains("not found")
                    .or(predicate::str::contains("privileges required"))
                    .or(predicate::str::contains("Administrator"))
                    .or(predicate::str::contains("No such file"))
                    .or(predicate::str::contains("DESTRUCTIVE")),
            );
    }
}

#[test]
fn test_benchmark_block_size_parsing() {
    // Test various valid block size formats
    for block_size in ["4K", "64K", "1M", "4M", "16M", "64M"] {
        engraver()
            .args([
                "benchmark",
                "/dev/nonexistent",
                "--block-size",
                block_size,
                "--yes",
            ])
            .assert()
            .failure()
            // Should fail for device reasons, not block size parsing
            .stderr(
                predicate::str::contains("not found")
                    .or(predicate::str::contains("privileges required"))
                    .or(predicate::str::contains("Administrator"))
                    .or(predicate::str::contains("No such file"))
                    .or(predicate::str::contains("DESTRUCTIVE")),
            );
    }
}

#[test]
fn test_benchmark_test_block_sizes_parsing() {
    // Test comma-separated block sizes
    engraver()
        .args([
            "benchmark",
            "/dev/nonexistent",
            "--test-block-sizes",
            "4K,64K,1M,4M,16M",
            "--yes",
        ])
        .assert()
        .failure()
        // Should fail for device reasons, not parsing
        .stderr(
            predicate::str::contains("not found")
                .or(predicate::str::contains("privileges required"))
                .or(predicate::str::contains("Administrator"))
                .or(predicate::str::contains("No such file"))
                .or(predicate::str::contains("DESTRUCTIVE")),
        );
}

#[test]
fn test_benchmark_config_settings_loaded() {
    let (_temp_dir, config_file) = setup_config_test();

    // Create a config with benchmark settings
    fs::write(
        &config_file,
        r#"
[benchmark]
block_size = "16M"
test_size = "512M"
pattern = "random"
passes = 3
"#,
    )
    .unwrap();

    // When running with the config, should use config values as defaults
    // (will fail for device reasons, but validates config is loaded)
    engraver()
        .args([
            "--config-file",
            config_file.to_str().unwrap(),
            "benchmark",
            "/dev/nonexistent",
            "--yes",
        ])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("not found")
                .or(predicate::str::contains("privileges required"))
                .or(predicate::str::contains("Administrator"))
                .or(predicate::str::contains("No such file"))
                .or(predicate::str::contains("DESTRUCTIVE")),
        );
}

#[test]
fn test_benchmark_cli_overrides_config() {
    let (_temp_dir, config_file) = setup_config_test();

    // Create a config with benchmark settings
    fs::write(
        &config_file,
        r#"
[benchmark]
block_size = "16M"
pattern = "random"
passes = 3
"#,
    )
    .unwrap();

    // CLI options should override config values
    // Using a different block size than config
    engraver()
        .args([
            "--config-file",
            config_file.to_str().unwrap(),
            "benchmark",
            "/dev/nonexistent",
            "--block-size",
            "4M",
            "--pattern",
            "zeros",
            "--yes",
        ])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("not found")
                .or(predicate::str::contains("privileges required"))
                .or(predicate::str::contains("Administrator"))
                .or(predicate::str::contains("No such file"))
                .or(predicate::str::contains("DESTRUCTIVE")),
        );
}

#[test]
fn test_benchmark_config_invalid_pattern_rejected() {
    let (_temp_dir, config_file) = setup_config_test();

    // Create a config with invalid benchmark pattern
    fs::write(
        &config_file,
        r#"
[benchmark]
pattern = "invalid_pattern"
"#,
    )
    .unwrap();

    // Should fail with pattern validation error
    engraver()
        .args([
            "--config-file",
            config_file.to_str().unwrap(),
            "benchmark",
            "/dev/nonexistent",
            "--yes",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Unknown pattern"));
}

#[test]
fn test_benchmark_config_invalid_block_size_rejected() {
    let (_temp_dir, config_file) = setup_config_test();

    // Create a config with invalid block size (not power of 2)
    fs::write(
        &config_file,
        r#"
[benchmark]
block_size = "3M"
"#,
    )
    .unwrap();

    // Should fail with block size validation error
    engraver()
        .args([
            "--config-file",
            config_file.to_str().unwrap(),
            "benchmark",
            "/dev/nonexistent",
            "--yes",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("must be a power of 2"));
}
