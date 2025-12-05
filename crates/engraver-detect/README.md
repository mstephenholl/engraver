# engraver-detect

Safe drive detection and enumeration with system drive protection.

## Features

- **Cross-platform**: Linux, macOS, and Windows support
- **Safety-first**: Multiple heuristics to identify and protect system drives
- **Rich metadata**: Drive type, vendor, model, partitions, mount points
- **Well-tested**: Comprehensive unit tests and fuzz testing

## Usage

```rust
use engraver_detect::{list_removable_drives, validate_target, DriveType};

// List only safe, removable drives
let drives = list_removable_drives()?;

for drive in &drives {
    println!("{} - {} ({}, {:?})", 
        drive.path,
        drive.display_name(),
        drive.size_display(),
        drive.drive_type
    );
}

// Validate a specific target before writing
let target = validate_target("/dev/sdb")?;
assert!(target.is_safe_target());
```

## Testing

### Unit Tests

```bash
# Run all unit tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run ignored integration tests (requires actual hardware)
cargo test -- --ignored
```

### Fuzz Testing

Requires `cargo-fuzz` (nightly Rust):

```bash
# Install cargo-fuzz
rustup install nightly
cargo +nightly install cargo-fuzz

# Run fuzz tests
cd fuzz
cargo +nightly fuzz run fuzz_plist_parser
cargo +nightly fuzz run fuzz_wmic_parser
cargo +nightly fuzz run fuzz_mount_parser
cargo +nightly fuzz run fuzz_format_bytes
```

## Safety Philosophy

This crate is safety-critical. It uses multiple heuristics to prevent accidental overwrites:

1. **Mount point detection**: Drives containing `/`, `/home`, `C:\`, etc. are marked as system drives
2. **Removable flag**: Non-removable internal drives are protected by default
3. **System partition detection**: EFI, Recovery, and system partitions trigger protection
4. **Conservative defaults**: When in doubt, drives are marked as unsafe

## Platform Support

| Platform | Detection Method | Status |
|----------|-----------------|--------|
| Linux    | `/sys/block/`, `/proc/mounts` | Full |
| macOS    | `diskutil` plist output | Full |
| Windows  | `wmic` CSV queries | Full |

## License

MIT
