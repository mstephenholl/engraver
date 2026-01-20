# Engraver

A fast, safe CLI tool for creating bootable USB drives, SD cards, and NVMe drives from ISO images.

Inspired by Balena Etcher, built for developers and automation.

> **New to Engraver?** See the [Quickstart Guide](docs/quickstart.md) to get started in 5 minutes.

## Features

- **Safety First** - System drive protection prevents accidental overwrites
- **Fast** - High-performance block writing with progress tracking
- **Reliable** - Post-write verification ensures data integrity
- **Remote Sources** - Write directly from HTTP/HTTPS URLs
- **Compression** - Supports .gz, .xz, .zst, and .bz2 compressed images
- **Resumable** - Resume interrupted writes with checkpoint support
- **USB Speed Detection** - Warns if USB 3.0 device is connected at USB 2.0 speed
- **Cross-Platform** - Linux, macOS, and Windows support
- **Developer-Friendly** - Clean CLI, JSON output, scriptable

## Installation

### Quick Install (Linux/macOS)

```bash
curl -fsSL https://raw.githubusercontent.com/mstephenholl/engraver/main/install.sh | bash
```

This installs the binary, shell completions, and man pages.

<details>
<summary>Verify the script before running (recommended)</summary>

```bash
# Download and inspect
curl -fsSL https://raw.githubusercontent.com/mstephenholl/engraver/main/install.sh -o install.sh
less install.sh

# Verify checksum
echo "d691f039c98a9598368532747504f8f1d9362977fe5897f96c7111e7de6ddd50  install.sh" | sha256sum -c

# Run after inspection
bash install.sh
```

</details>

### From Source

```bash
cargo install --path crates/engraver-cli
```

### Pre-built Binaries

Download from [Releases](https://github.com/mstephenholl/engraver/releases).

## Usage

```bash
# List available drives
engraver list

# Write an ISO to a USB drive
engraver write ubuntu.iso /dev/sdb

# Write directly from a URL
engraver write https://releases.ubuntu.com/24.04/ubuntu-24.04-desktop-amd64.iso /dev/sdb

# Write a compressed image (auto-detected)
engraver write ubuntu.iso.xz /dev/sdb

# Write with post-write verification (recommended)
engraver write ubuntu.iso /dev/sdb --verify

# Skip confirmation prompt (for scripts)
engraver write ubuntu.iso /dev/sdb -y

# Silent mode (no output, implies -y)
engraver write ubuntu.iso /dev/sdb --silent

# Verify a device against an image
engraver verify ubuntu.iso /dev/sdb

# Calculate checksum (supports sha256, sha512, md5, crc32)
engraver checksum ubuntu.iso --algorithm sha256

# Enable checkpointing for resume support
engraver write ubuntu.iso /dev/sdb --checkpoint

# Resume an interrupted write
engraver write ubuntu.iso /dev/sdb --resume
```

## Resume Support

Engraver supports resuming interrupted writes. If a write is cancelled (Ctrl+C) or fails due to an error, a checkpoint is saved automatically when using `--checkpoint` or `--resume`.

```bash
# Start a write with checkpointing enabled
engraver write large-image.iso /dev/sdb --checkpoint

# If interrupted, resume from where it left off
engraver write large-image.iso /dev/sdb --resume
```

**Resume limitations:**
- Local files: Always resumable (seekable)
- HTTP/HTTPS sources: Resumable if the server supports Range headers
- Compressed files (.gz, .xz, .zst, .bz2): Cannot be resumed

Checkpoints are stored in:
- Linux/macOS: `~/.local/state/engraver/checkpoints/`
- Windows: `%LOCALAPPDATA%\engraver\checkpoints\`

## Remote Sources

Engraver can write images directly from HTTP/HTTPS URLs without downloading first:

```bash
# Write from a URL
engraver write https://example.com/image.iso /dev/sdb

# Combine with verification
engraver write https://example.com/image.iso /dev/sdb --verify

# Resume support works with URLs (if server supports Range headers)
engraver write https://example.com/large-image.iso /dev/sdb --checkpoint
```

## Compression Support

Compressed images are automatically detected and decompressed during write:

| Format | Extensions |
|--------|------------|
| Gzip | `.gz`, `.gzip` |
| XZ | `.xz` |
| Zstandard | `.zst`, `.zstd` |
| Bzip2 | `.bz2`, `.bzip2` |

```bash
# Write compressed images (format auto-detected by extension)
engraver write ubuntu.iso.xz /dev/sdb
engraver write raspbian.img.gz /dev/sdb
engraver write archlinux.iso.zst /dev/sdb
```

Compressed images cannot be resumed if interrupted.

## Shell Completions

Engraver can generate shell completions for tab-completion of commands and arguments.

```bash
# Bash - add to ~/.bashrc
engraver completions bash >> ~/.bashrc

# Zsh - add to ~/.zshrc or a completions directory
engraver completions zsh >> ~/.zshrc

# Fish
engraver completions fish > ~/.config/fish/completions/engraver.fish

# PowerShell - add to your profile
engraver completions powershell >> $PROFILE

# Elvish
engraver completions elvish >> ~/.elvish/rc.elv
```

## Configuration

Engraver supports a configuration file for persistent settings. This allows you to set default values for frequently used options.

### Configuration File Location

- Linux/macOS: `~/.config/engraver/engraver_config.toml`
- Windows: `%APPDATA%\engraver\engraver_config.toml`

### Managing Configuration

```bash
# View current settings
engraver config

# View settings as JSON
engraver config --json

# Show config file path
engraver config --path

# Create a new config file with defaults
engraver config --init
```

### Example Configuration

```toml
[write]
block_size = "4M"
verify = true
checkpoint = false

[checksum]
algorithm = "sha256"
auto_detect = false

[behavior]
skip_confirmation = false
quiet = false
```

### Configuration Options

| Section | Option | Description | Default |
|---------|--------|-------------|---------|
| `[write]` | `block_size` | Default block size for writes | `"4M"` |
| `[write]` | `verify` | Always verify writes | `false` |
| `[write]` | `checkpoint` | Enable checkpointing by default | `false` |
| `[checksum]` | `algorithm` | Default checksum algorithm | `"sha256"` |
| `[checksum]` | `auto_detect` | Auto-detect checksum files | `false` |
| `[behavior]` | `skip_confirmation` | Skip confirmation prompts | `false` |
| `[behavior]` | `quiet` | Suppress non-error output | `false` |

Command-line flags always override configuration file settings.

## Safety

Engraver includes multiple safety mechanisms:

1. **System drive detection** - Refuses to write to drives containing system partitions
2. **Removable-only by default** - Only shows removable drives unless `--all` is specified
3. **Confirmation prompts** - Requires explicit confirmation before writing
4. **Verification** - Optional post-write verification with `--verify`

## Development

```bash
# Run tests
cargo test

# Run clippy
cargo clippy -- -D warnings

# Build release
cargo build --release

# Run CLI
cargo r -- list
```

## License

MIT License - see [LICENSE](LICENSE) for details.
