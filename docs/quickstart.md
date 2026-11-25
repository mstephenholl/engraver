# Quickstart Guide

Get up and running with Engraver in under 5 minutes.

## Installation

### Linux/macOS (Recommended)

```bash
curl -fsSL https://raw.githubusercontent.com/mstephenholl/engraver/main/install.sh | bash
```

### From Source

```bash
cargo install --path crates/engraver-cli
```

### Pre-built Binaries

Download from [Releases](https://github.com/mstephenholl/engraver/releases).

## Basic Usage

### 1. List Available Drives

```bash
engraver list
```

Output shows all removable drives with their size and USB speed:

```
Available drives:
  /dev/sdb - SanDisk Ultra (32 GB) | USB 3.0 SuperSpeed
  /dev/sdc - Kingston DataTraveler (16 GB) | USB 2.0 High Speed (slow)
```

### 2. Write an Image

```bash
# Basic write
sudo engraver write ubuntu.iso /dev/sdb

# Write with verification (recommended)
sudo engraver write ubuntu.iso /dev/sdb --verify

# Skip confirmation prompt (for scripts)
sudo engraver write ubuntu.iso /dev/sdb -y
```

### 3. Write from URL

```bash
# Download and write in one step
sudo engraver write https://releases.ubuntu.com/24.04/ubuntu-24.04-desktop-amd64.iso /dev/sdb
```

### 4. Verify an Existing Drive

```bash
sudo engraver verify ubuntu.iso /dev/sdb
```

## Common Options

| Option | Description |
|--------|-------------|
| `--verify` | Verify after writing (hashes during write, then reads back) |
| `-y` | Skip confirmation prompt |
| `-q` | Hide progress and status output (still asks for confirmation) |
| `--silent` | Like `-q`, and skip confirmation (implies -y) |
| `--checkpoint` | Enable resume support |
| `--resume` | Resume interrupted write |
| `--auto-checksum` | Auto-detect .sha256/.md5 checksum files |
| `--all` | Show all drives (including non-removable) |

## Working with Compressed Images

Engraver automatically detects and decompresses:
- `.gz` / `.gzip` (Gzip)
- `.xz` (XZ)
- `.zst` / `.zstd` (Zstandard)
- `.bz2` / `.bzip2` (Bzip2)

```bash
# Just use the compressed file directly
sudo engraver write ubuntu.iso.xz /dev/sdb
```

## Resuming Interrupted Writes

```bash
# Start with checkpointing
sudo engraver write large-image.iso /dev/sdb --checkpoint

# If interrupted (Ctrl+C), resume later
sudo engraver write large-image.iso /dev/sdb --resume
```

**Note:** Compressed files cannot be resumed.

## Checksum Verification

Verify source integrity before writing:

```bash
# Provide checksum manually
sudo engraver write ubuntu.iso /dev/sdb --checksum abc123...

# Auto-detect from companion files (e.g., ubuntu.iso.sha256, SHA256SUMS)
sudo engraver write ubuntu.iso /dev/sdb --auto-checksum

# Enable auto-detection by default in config
engraver config --init
# Then edit the file that `engraver config --path` prints:
# [checksum]
# auto_detect = true
```

Under `sudo`, Engraver usually reads root's configuration file instead of this one. To use this file, see [Configuration](#configuration).

Supported checksum file formats:
- Direct: `image.iso.sha256`, `image.iso.sha512`, `image.iso.md5`
- SUMS files: `SHA256SUMS`, `SHA512SUMS`, `MD5SUMS` (in same directory)

## Configuration

Create a configuration file to set persistent defaults:

```bash
# Create default config file
engraver config --init

# View current settings
engraver config
```

Configuration file location:

- Linux: `~/.config/engraver/engraver_config.toml`, or `$XDG_CONFIG_HOME/engraver/engraver_config.toml` if you set `XDG_CONFIG_HOME`
- macOS: `~/Library/Application Support/engraver/engraver_config.toml`
- Windows: `%APPDATA%\engraver\engraver_config.toml`

`engraver config --path` prints the path Engraver uses.

`sudo` usually sets `HOME` to root's home directory, so `sudo engraver` reads root's configuration file, not yours. To use your file under `sudo`, pass its path with `--config-file`:

```bash
sudo engraver --config-file "$(engraver config --path)" write ubuntu.iso /dev/sdb
```

Example settings:
```toml
[write]
verify = true      # Always verify writes

[checksum]
algorithm = "sha256"
```

## Safety Features

Engraver protects you from accidents:

1. **System drive protection** - Refuses to write to drives with system partitions
2. **Removable-only default** - Only shows removable drives unless `--all` is used
3. **Confirmation prompts** - Always asks before writing (unless `-y` is used)
4. **USB speed warnings** - Alerts if a USB 3.0 device is running at USB 2.0 speed

## Next Steps

- See [README.md](../README.md) for full documentation
- Check [CONTRIBUTING.md](CONTRIBUTING.md) to contribute
- Read [architecture/overview.md](architecture/overview.md) for technical details
