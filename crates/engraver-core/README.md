# engraver-core

Core library for the Engraver disk imaging tool.

## Features

- **High-performance writer**: Block-based writing with configurable sizes (4KB-64MB)
- **Progress tracking**: Real-time speed calculation and ETA estimation
- **Source handling**: Local files, HTTP/HTTPS URLs, compressed archives, cloud storage
- **Verification**: Parallel write verification — hashes source during write, reads back target to compare (SHA-256, SHA-512, MD5, CRC32)
- **Resume support**: Checkpoint-based resumption of interrupted writes
- **Benchmark**: Drive performance testing with configurable patterns
- **Partition inspection**: MBR and GPT partition table parsing

## Modules

| Module | Description |
|--------|-------------|
| `writer` | High-performance block writer with progress callbacks |
| `source` | Multi-source abstraction (local, remote, compressed, cloud) |
| `verifier` | Post-write verification and checksum validation |
| `benchmark` | Drive performance testing with configurable patterns |
| `resume` | Checkpoint-based write resumption |
| `partition` | Partition table parsing (MBR/GPT) |
| `settings` | Persistent user configuration (TOML-based) |
| `config` | Runtime configuration |
| `error` | Comprehensive error types |

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `compression` | Yes | Gzip, XZ, Zstandard, Bzip2 decompression |
| `remote` | Yes | HTTP/HTTPS URL support |
| `checksum` | Yes | SHA-256, SHA-512, MD5, CRC32 checksums |
| `partition-info` | Yes | Partition table inspection |
| `s3` | No | AWS S3 and S3-compatible storage |
| `gcs` | No | Google Cloud Storage |
| `azure` | No | Azure Blob Storage |
| `cloud` | No | Enable all cloud storage providers |

## Usage

Each example is the body of a function that returns `Result<(), Box<dyn std::error::Error>>`.

### Basic Write

```rust
use engraver_core::Writer;
use std::io::Cursor;

let source = Cursor::new(vec![0xABu8; 1024 * 1024]); // 1 MB
let target = Cursor::new(vec![0u8; 1024 * 1024]);

let mut writer = Writer::new();
let result = writer.write(source, target, 1024 * 1024)?;

println!("Wrote {} bytes", result.bytes_written);
```

### With Progress Tracking

```rust
use engraver_core::{Writer, WriteConfig};
use std::io::Cursor;

let source_size = 8 * 1024 * 1024; // 8 MB
let source = Cursor::new(vec![0xABu8; source_size]);
let target = Cursor::new(vec![0u8; source_size]);

let config = WriteConfig::new()
    .block_size(4 * 1024 * 1024)  // 4 MB blocks
    .sync_on_complete(true);

let mut writer = Writer::with_config(config)
    .on_progress(|progress| {
        println!(
            "{:.1}% - {} - ETA: {}",
            progress.percentage(),
            progress.speed_display(),
            progress.eta_display()
        );
    });

let result = writer.write(source, target, source_size as u64)?;
println!("Wrote {} bytes", result.bytes_written);
```

### With Cancellation

```rust
use engraver_core::{Error, Writer};
use std::io::Cursor;
use std::sync::atomic::Ordering;
use std::thread;

let size = 64 * 1024 * 1024; // 64 MB
let source = Cursor::new(vec![0xABu8; size]);
let target = Cursor::new(vec![0u8; size]);

let mut writer = Writer::new();
let cancel_handle = writer.cancel_handle();

// Spawn write in background
let handle = thread::spawn(move || {
    writer.write(source, target, size as u64)
});

// Cancel after some condition
cancel_handle.store(true, Ordering::SeqCst);

// The write returns Err(Error::Cancelled) if it sees the flag before it finishes
match handle.join().unwrap() {
    Err(Error::Cancelled) => println!("Write cancelled"),
    Err(e) => return Err(e.into()),
    Ok(result) => println!("Write finished first: {} bytes", result.bytes_written),
}
```

### Source Handling

```rust
use engraver_core::source::{Source, detect_source_type, SourceType};

// Auto-detect source type
let source_type = detect_source_type("image.iso.xz");
assert_eq!(source_type, SourceType::Xz);

// Open a source (handles local, remote, compressed)
let source = Source::open("https://example.com/image.iso")?;
println!("Size: {:?}", source.info().size);
```

### Checksum Verification

```rust
use engraver_core::{Verifier, ChecksumAlgorithm};
use std::fs::File;

let mut verifier = Verifier::new();

// Calculate checksum
let mut file = File::open("image.iso")?;
let checksum = verifier.calculate_checksum(&mut file, ChecksumAlgorithm::Sha256, None)?;
println!("SHA-256: {checksum}");

// Verify against an expected hash. A mismatch returns Err(Error::ChecksumMismatch).
let expected = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
let mut file = File::open("image.iso")?;
verifier.verify_checksum(&mut file, ChecksumAlgorithm::Sha256, expected, None)?;

// Auto-detect checksum from companion files (.sha256, SHA256SUMS, etc.)
if let Some(detected) = engraver_core::verifier::auto_detect_checksum("image.iso") {
    println!("Found {} checksum: {}", detected.algorithm, detected.checksum);
}
```

### Partition Inspection

```rust
use engraver_core::partition::inspect_partitions;
use std::fs::File;

let mut file = File::open("image.iso")?;
let info = inspect_partitions(&mut file)?;
println!("Partition table: {:?}", info.table_type);
for partition in &info.partitions {
    println!("  Partition {}: {} bytes", partition.number, partition.size);
}
```

## Configuration Options

| Option | Default | Description |
|--------|---------|-------------|
| `block_size` | 32 MB | Size of read/write blocks |
| `sync_each_block` | false | fsync after each block |
| `sync_on_complete` | true | fsync when write completes |
| `retry_attempts` | 3 | Number of retry attempts |
| `retry_delay` | 100ms | Wait before the first retry. Each retry after that waits twice as long, up to 8 times this value |
| `verify` | false | Saved in a resume checkpoint. `Writer` does not read it |
| `checksum_algorithm` | None | Hash the source during the write. `Writer::write_and_verify` then reads the target back and compares hashes |

## Progress Information

The `WriteProgress` struct provides:

- `phase`: Current operation phase (`Writing` or `Verifying`)
- `bytes_written` / `total_bytes`: Progress tracking
- `speed_bps`: Current write speed in bytes/second
- `eta_seconds`: Estimated time remaining
- `current_block` / `total_blocks`: Block progress
- `retry_count`: Number of retries so far
- `percentage()`: Completion percentage (0.0 - 100.0)
- `speed_display()`: Human-readable speed (e.g., "45.2 MB/s")
- `eta_display()`: Human-readable ETA (e.g., "2m 30s")

## Testing

```bash
# Run unit tests
cargo test -p engraver-core

# Run with output
cargo test -p engraver-core -- --nocapture

# Run integration tests
cargo test -p engraver-core --test integration_tests

# Run benchmarks
cargo bench -p engraver-core
```

### Fuzz Testing

```bash
cd crates/engraver-core/fuzz
cargo +nightly fuzz run fuzz_format_functions -- -max_total_time=60
cargo +nightly fuzz run fuzz_write_config -- -max_total_time=60
cargo +nightly fuzz run fuzz_write_progress -- -max_total_time=60
```

## License

MIT
