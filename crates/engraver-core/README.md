# engraver-core

Core library for the Engraver disk imaging tool.

## Features

- **High-performance writer**: Block-based writing with configurable sizes (4KB-64MB)
- **Progress tracking**: Real-time speed calculation and ETA estimation
- **Source handling**: Local files, HTTP/HTTPS URLs, compressed archives, cloud storage
- **Verification**: Post-write read-back verification with multiple checksum algorithms
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

### Basic Write

```rust
use engraver_core::{Writer, WriteConfig};
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

let result = writer.write(source, target, source_size)?;
```

### With Cancellation

```rust
use engraver_core::Writer;
use std::sync::atomic::Ordering;
use std::thread;

let mut writer = Writer::new();
let cancel_handle = writer.cancel_handle();

// Spawn write in background
let handle = thread::spawn(move || {
    writer.write(source, target, size)
});

// Cancel after some condition
cancel_handle.store(true, Ordering::SeqCst);

let result = handle.join().unwrap();
// result will be Err(Error::Cancelled)
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

let mut verifier = Verifier::new();

// Calculate checksum
let checksum = verifier.calculate_checksum(&mut reader, ChecksumAlgorithm::Sha256, None)?;

// Verify against expected
verifier.verify_checksum(&mut reader, ChecksumAlgorithm::Sha256, &expected, None)?;

// Auto-detect checksum from companion files (.sha256, SHA256SUMS, etc.)
let detected = engraver_core::verifier::auto_detect_checksum("image.iso")?;
```

### Partition Inspection

```rust
use engraver_core::partition::inspect_partitions;

let info = inspect_partitions("image.iso")?;
println!("Partition table: {:?}", info.table_type);
for partition in &info.partitions {
    println!("  Partition {}: {} bytes", partition.number, partition.size);
}
```

## Configuration Options

| Option | Default | Description |
|--------|---------|-------------|
| `block_size` | 4 MB | Size of read/write blocks |
| `sync_each_block` | false | fsync after each block |
| `sync_on_complete` | true | fsync when write completes |
| `retry_attempts` | 3 | Number of retry attempts |
| `retry_delay` | 100ms | Delay between retries |
| `verify` | false | Read-back verification |

## Progress Information

The `WriteProgress` struct provides:

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
