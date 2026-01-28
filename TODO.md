# Engraver TODO

Planned features, improvements, and development roadmap for Engraver.

**Last updated**: 2026-01-27

---

## Features

- [x] **Resume/retry support for interrupted writes**
  - Checkpoint files saved on Ctrl+C or errors
  - `--checkpoint` and `--resume` CLI flags
  - Works with local files and HTTP sources (with Range header support)
  - Compressed sources cannot be resumed

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

- [x] **Automatic checksum file detection**
  - Auto-detect .sha256, .sha512, .md5 files alongside ISOs
  - `auto_detect_checksum()` function in verifier.rs
  - CLI flag: `--auto-checksum`, config option: `auto_detect = true`
  - Searches for `.sha256`, `.sha512`, `.md5`, `.sha256sum` files

- [ ] **Partition table inspection**
  - Display partition layout of source images
  - Show what will be written before confirmation

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

---

## Platform Support

- [ ] **FreeBSD support**
  - Cross-compile using `x86_64-unknown-freebsd` target
  - Implement drive detection via `geom` subsystem
  - Device paths: `/dev/da0`, `/dev/ada0`, etc.
  - CI testing via `vmactions/freebsd-vm` GitHub Action
  - Lower priority - implement based on user demand

---

## CI/CD & Security

### Completed

- [x] **Dependabot configuration** - `.github/dependabot.yml` created
- [x] **cargo-audit in CI** - Security vulnerability scanning active
- [x] **Repository URLs fixed** - Updated from `yourusername` to `mstephenholl`
- [x] **Code coverage with Codecov** - tarpaulin + codecov.io integration
- [x] **MSRV validation job** - Testing against Rust 1.85
- [x] **cargo-deny for license compliance** - `deny.toml` created with license allowlist

### High Priority

- [ ] **Add sanitizer testing to CI**
  - ASAN/MSAN jobs for engraver-platform unsafe code
  - Catches memory safety issues

- [ ] **Add supply chain security**
  - Use `--locked` flag in CI builds
  - Hash verification for dependencies

- [ ] **Add secret scanning**
  - truffleHog or similar GitHub Action for credential detection

### Medium Priority

- [ ] **Add SBOM generation to releases**
  - cargo-sbom in release workflow
  - Software Bill of Materials for compliance

- [ ] **Parallelize integration tests**
  - Remove sequential dependency on unit tests

- [ ] **Extract reusable workflows**
  - Create `_build.yml`, `_test.yml` for DRY CI configuration

- [ ] **Add performance benchmarking CI**
  - cargo-criterion for regression detection

### Low Priority

- [ ] **Sign release artifacts**
  - Add GPG signing to release workflow

- [ ] **Beta/nightly Rust CI testing**
  - Early warning for upstream breakage

---

## Dependency Management

- [ ] **Tighten cargo-deny configuration**
  - Review and reduce permissiveness in `deny.toml`
  - Consider stricter license allow-list
  - Evaluate changing `multiple-versions = "warn"` to `"deny"`
  - Review `wildcards = "allow"` setting for workspace dependencies

- [ ] **Audit and remediate unmaintained dependencies**
  - `number_prefix` (RUSTSEC-2025-0119) - transitive via `indicatif`
    - Track upstream: wait for indicatif to update or find alternative
  - Periodically review advisory-db ignore list
  - Consider pinning or replacing problematic transitive deps

---

## Improvements

- [ ] **Better error messages for common failures**
- [ ] **More detailed progress information** (blocks written, retries, etc.)
- [x] **Add warning logs for silent errors**
  - Added `tracing::warn!()` in detect crate for recoverable errors
  - Implemented in linux.rs, macos.rs error paths (e.g., size parsing failures)
- [ ] **GUI implementation (engraver-gui crate)**
  - Placeholder exists, planned frameworks: iced or Tauri
- [ ] **Windows-specific optimizations**
- [ ] **macOS-specific optimizations**

---

## Testing

- [x] **CLI unit tests** - Comprehensive test coverage across all crates
  - 589+ unit tests across the codebase (based on `#[test]` count)
  - Integration tests in cli, core, detect, and platform crates
  - Added tests for list.rs, benchmark.rs, checksum.rs utility functions

- [ ] **Integration tests for actual write operations**
  - Test with virtual block devices or disk images
  - End-to-end write and verify workflows

- [ ] **Integration tests for verify operations**

- [ ] **HTTP source integration tests** (with mock server)

- [ ] **Compression decompression tests** with real compressed images

- [ ] **Expand fuzz targets**
  - Additional coverage of edge cases (currently 12 targets)

---

## Documentation

- [x] **Contributing guide** (CONTRIBUTING.md)
- [x] **Architecture documentation** (docs/architecture/overview.md)

- [ ] **Man page improvements**
- [ ] **More examples in README**
- [ ] **Fix shell completion documentation**
  - Reconcile README.md and CLI/README.md install paths
- [ ] **Add benchmark to man page list**
  - Update CLI/README.md to include `engraver-benchmark.1`
- [ ] **Update CLI Cargo.toml description**
  - Add "SD cards, NVMe" to description

---

## Code Quality Assessment

### Strengths

- No panicking `unwrap()` calls in production code
- Excellent error handling with custom error types (`thiserror`)
- Proper constant definitions throughout
- Safe command execution (no shell injection vectors)
- Well-justified unsafe code with documentation
- Reasonable performance patterns

### Minor Improvements

- [ ] Consider `String::with_capacity()` in label decoding (minor optimization)

---

## Codebase Statistics

| Metric | Value |
|--------|-------|
| Total Rust files | 44 |
| Lines of code (src) | ~13,200 |
| Main crates | 5 |
| Platforms supported | 3 |
| Compression formats | 4 |
| Checksum algorithms | 4 |
| Fuzzing targets | 12 |
| Test count | 589+ |

---

## Progress Metrics

| Metric | Status |
|--------|--------|
| Dead code annotations | 2 (GUI only) |
| CI security checks | 2 (audit + deny) |
| Code coverage | Tracked via Codecov (target >70%) |
| Dependabot enabled | Yes |
| CLI unit test coverage | 589+ tests |
