# engraver-core

Core library for the Engraver disk imaging tool.

## Features

- **High-performance writer**: Block-based writing with configurable sizes
- **Progress tracking**: Real-time speed calculation and ETA estimation
- **Retry logic**: Automatic retry on transient errors
- **Cancellation support**: Cooperative cancellation via atomic flag
- **Verification**: Post-write read-back verification

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
