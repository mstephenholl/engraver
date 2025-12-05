# engraver-platform

Platform-specific raw device I/O for the Engraver disk imaging tool.

## Features

- **Direct I/O**: Bypasses OS page cache for reliable writes
- **Cross-platform**: Linux, macOS, and Windows support
- **Alignment handling**: Automatic buffer alignment for direct I/O
- **Volume management**: Unmount filesystems before writing
- **Well-tested**: Comprehensive unit tests and fuzz testing

## Platform Support

| Platform | Direct I/O | Unmount | Privilege Check |
|----------|-----------|---------|-----------------|
| Linux    | O_DIRECT  | umount  | geteuid() == 0  |
| macOS    | F_NOCACHE + rdisk | diskutil | geteuid() == 0 |
| Windows  | FILE_FLAG_NO_BUFFERING | PowerShell | TokenElevation |

## Usage

```rust
use engraver_platform::{open_device, unmount_device, OpenOptions};

// Unmount the device first
unmount_device("/dev/sdb")?;

// Open for direct I/O
let options = OpenOptions::new()
    .direct_io(true)
    .read(true)
    .write(true)
    .block_size(4096);

let mut device = open_device("/dev/sdb", options)?;

// Write data (aligned to block size for direct I/O)
let data = vec![0u8; 4096];
device.write_at(0, &data)?;

// Sync to ensure data is on disk
device.sync()?;

// Read it back
let mut buffer = vec![0u8; 4096];
device.read_at(0, &mut buffer)?;
```

## Alignment

Direct I/O requires aligned buffers and offsets. Use the alignment helpers:

```rust
use engraver_platform::{align_up, align_down, is_aligned};

let block_size = 4096;

// Align a size up to block boundary
let size = align_up(5000, block_size); // = 8192

// Check if aligned
assert!(is_aligned(size, block_size));
```

## Testing

```bash
# Run unit tests
cargo test

# Run integration tests (may need sudo for device tests)
cargo test -- --nocapture

# Run ignored tests (require actual hardware)
sudo cargo test -- --ignored
```

### Fuzz Testing

```bash
# Install cargo-fuzz
rustup install nightly
cargo +nightly install cargo-fuzz

# Run fuzz tests
cd fuzz
cargo +nightly fuzz run fuzz_alignment -- -max_total_time=60
cargo +nightly fuzz run fuzz_path_normalize -- -max_total_time=60
```

## Safety Considerations

**Warning: This crate writes directly to block devices.** Incorrect use can destroy data.

- Always validate device paths using `engraver-detect` first
- Unmount filesystems before writing
- Verify writes after completion
- Use with elevated privileges (root/Administrator)

## License

MIT
