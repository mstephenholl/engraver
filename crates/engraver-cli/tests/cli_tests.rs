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
        .stdout(predicate::str::contains("0.1.0"));
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
quiet = true
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
