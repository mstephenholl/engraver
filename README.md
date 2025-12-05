# Engraver 🔥

A fast, safe CLI tool for creating bootable USB drives, SD cards, and NVMe drives from ISO images.

Inspired by Balena Etcher, built for developers and automation.

## Features

- 🛡️ **Safety First** - System drive protection prevents accidental overwrites
- ⚡ **Fast** - High-performance block writing with progress tracking
- ✅ **Reliable** - Post-write verification ensures data integrity
- 🖥️ **Cross-Platform** - Linux, macOS, and Windows support
- 🔧 **Developer-Friendly** - Clean CLI, JSON output, scriptable

## Installation

### From Source

```bash
cargo install --path crates/engraver-cli
```

### Pre-built Binaries

Download from [Releases](https://github.com/yourusername/engraver/releases).

## Usage

```bash
# List available drives
engraver list

# Write an ISO to a USB drive
engraver write --source ubuntu.iso --target /dev/sdb

# Write without verification (faster, less safe)
engraver write --source image.img --target /dev/sdb --no-verify

# Verify a device against an image
engraver verify --source ubuntu.iso --target /dev/sdb

# Calculate checksum
engraver checksum ubuntu.iso --algorithm sha256
```

## Safety

Engraver includes multiple safety mechanisms:

1. **System drive detection** - Refuses to write to drives containing system partitions
2. **Removable-only by default** - Only shows removable drives unless `--all` is specified
3. **Confirmation prompts** - Requires explicit confirmation before writing
4. **Verification** - Validates writes by default

## Development

```bash
# Run tests
cargo test

# Run clippy
cargo lint

# Build release
cargo build --release

# Run CLI
cargo r -- list
```

## License

MIT License - see [LICENSE](LICENSE) for details.
