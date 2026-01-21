# Engraver TODO

Planned features and improvements for Engraver.

## Features

- [x] **Resume/retry support for interrupted writes**
  - Checkpoint files saved on Ctrl+C or errors
  - `--checkpoint` and `--resume` CLI flags
  - Works with local files and HTTP sources (with Range header support)
  - Compressed sources cannot be resumed

- [ ] **Parallel verification (checksum while writing)**
  - Calculate checksum during write operation instead of separate pass
  - Reduces total time for write+verify workflow

- [ ] **Multi-drive support**
  - Write to multiple drives simultaneously
  - Useful for creating multiple bootable USBs at once

- [ ] **Image caching**
  - Cache downloaded images locally for repeated writes
  - Automatic cache management with size limits

- [ ] **Progress webhooks/callbacks**
  - HTTP webhook support for progress updates
  - Integration with CI/CD pipelines and automation tools

- [ ] **Automatic checksum file detection**
  - Auto-detect .sha256, .sha512, .md5 files alongside ISOs
  - Automatically verify source integrity before writing

- [ ] **Partition table inspection**
  - Display partition layout of source images
  - Show what will be written before confirmation

- [x] **USB device speed detection**
  - Detect USB 2.0 vs 3.0 connection speeds
  - Warn if device is connected at slower speed than capable
  - Shows speed in `engraver list` output
  - Warns during `engraver write` if using slow USB 2.0

- [x] **Write speed benchmarking mode**
  - `engraver benchmark /dev/sdb` to test drive write speed
  - Supports custom test size, block size, and data patterns
  - `--test-block-sizes` to find optimal block size
  - Color-coded progress bar (red→yellow→green→blue)

- [x] **Configuration file support**
  - `~/.config/engraver/config.toml` for default settings
  - `engraver config --init` to create, `engraver config` to view
  - `--config-file` flag for custom config path

- [ ] **Cloud storage support (S3/GCS/Azure Blob)**
  - Download ISOs directly from cloud storage buckets
  - Support for presigned URLs and credential-based authentication
  - Resume support via Range headers (same as HTTP)
  - Optional feature flag to keep binary size small

- [ ] **BitTorrent/Magnet link support**
  - Download ISOs via BitTorrent protocol
  - Support for magnet links (common for Linux distros)
  - Built-in piece verification for integrity
  - Optional feature flag for opt-in compilation

- [ ] **Enhanced HTTP features**
  - Proxy support (HTTP/HTTPS/SOCKS5)
  - Custom headers for authentication or CDN access
  - Basic/Digest/Bearer token authentication
  - Configurable timeouts and retry policies

## Platform Support

- [ ] **FreeBSD support**
  - Cross-compile using `x86_64-unknown-freebsd` target
  - Implement drive detection via `geom` subsystem
  - Device paths: `/dev/da0`, `/dev/ada0`, etc.
  - CI testing via `vmactions/freebsd-vm` GitHub Action
  - Lower priority - implement based on user demand

## Improvements

- [ ] Better error messages for common failures
- [ ] More detailed progress information (blocks written, retries, etc.)
- [ ] GUI implementation (engraver-gui crate)
  - Placeholder exists, planned frameworks: iced or Tauri
- [ ] Windows-specific optimizations
- [ ] macOS-specific optimizations

## Testing

- [ ] Integration tests for actual write operations
  - Test with virtual block devices or disk images
  - End-to-end write and verify workflows
- [ ] Integration tests for verify operations
- [ ] HTTP source integration tests (with mock server)
- [ ] Compression decompression tests with real compressed images

## Documentation

- [ ] Man page improvements
- [ ] More examples in README
- [x] Contributing guide (CONTRIBUTING.md)
- [x] Architecture documentation (docs/architecture/overview.md)
