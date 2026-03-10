# Contributing to Engraver

Thank you for your interest in contributing!

## Getting Started

1. Fork the repository
2. Clone your fork
3. Create a feature branch
4. Make your changes
5. Run tests: `cargo test`
6. Run linting: `cargo clippy -- -D warnings`
7. Submit a pull request

## Pre-Commit Hooks

This project uses [cargo-husky](https://github.com/rhysd/cargo-husky) for pre-commit hooks.
Hooks are **automatically installed** when you run `cargo build` or `cargo test`.

The pre-commit hook runs:
- `cargo fmt --check` - Verify code formatting
- `cargo clippy --all-targets --all-features -- -D warnings` - Lint checking
- `cargo test --lib` - Unit tests

To bypass hooks temporarily (not recommended):
```bash
git commit --no-verify
```

## Testing

### Unit and Integration Tests

Run the full test suite (no special hardware required):

```bash
cargo test --workspace
```

Run tests for a specific crate:

```bash
cargo test -p engraver-core
cargo test -p engraver -- write   # filter by test name
```

### Device Workflow Tests

End-to-end tests that validate write, verify, and erase workflows against a
**real removable drive**. These tests are destructive and `#[ignore]`d by
default — they never run in CI or during normal `cargo test`.

**Requirements:**
- A removable drive (USB stick, SD card) connected to the system
- Root/admin privileges
- The `ENGRAVER_TEST_DEVICE` environment variable set to the device path

**Running:**

```bash
# Linux
sudo ENGRAVER_TEST_DEVICE=/dev/sdX cargo test -p engraver --test device_tests -- --ignored

# macOS
sudo ENGRAVER_TEST_DEVICE=/dev/disk4 cargo test -p engraver --test device_tests -- --ignored
```

> **WARNING**: All data on the test device will be permanently destroyed!

The device tests cover:
- **Write** — writing a test image to the device
- **Write + verify** — writing with the `--verify` flag
- **Standalone verify** — writing then verifying with a separate `verify` command
- **Block size variations** — writing with 4K, 64K, and 1M block sizes
- **Erase** — zero-filling the device
- **Mismatch detection** — verifying that a wrong image is rejected
- **Full lifecycle** — write → verify → erase → verify zeros
- **Checkpoint** — writing with `--checkpoint` enabled

One safety-check test (`test_erase_rejects_nonexistent_device`) runs without a
device and without `#[ignore]`. Write rejection tests live in `cli_tests.rs`.

## Code Style

- Follow Rust conventions
- Run `cargo fmt` before committing
- Address all clippy warnings
- Add tests for new functionality
- Document public APIs

## Safety

This project deals with raw disk I/O. Safety is paramount:

- Never skip safety checks
- Test on virtual machines first
- Document any unsafe code thoroughly
- Consider edge cases carefully

## Pull Request Process

1. Update documentation as needed
2. Add unit, integration, and e2e tests for new features
3. Ensure CI passes
4. Request review from maintainers
