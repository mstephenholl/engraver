# Engraver CLI

A safe, fast command-line tool for creating bootable USB drives, SD cards, and NVMe drives.

## Features

- **Safe by default**: Refuses to write to system drives
- **Multiple sources**: Local files, URLs, compressed archives, cloud storage
- **Verification**: Optional post-write verification with multiple checksum algorithms
- **Progress display**: Real-time progress with speed and ETA
- **Resumable**: Checkpoint-based resume for interrupted writes
- **Cross-platform**: Linux, macOS, Windows support
- **Direct I/O**: Bypasses page cache for optimal write performance
- **USB speed detection**: Warns if USB 3.0 device runs at USB 2.0 speed

## Installation

```bash
cargo install engraver
```

Or build from source:

```bash
cd crates/engraver-cli
cargo build --release
```

## Commands

| Command | Description |
|---------|-------------|
| `list` | Display available drives |
| `write` | Write image to drive |
| `verify` | Verify drive against image |
| `checksum` | Calculate file checksum |
| `benchmark` | Test drive write speed |
| `config` | Manage configuration |
| `completions` | Generate shell completions |
| `mangen` | Generate man pages |

## Usage

### List Available Drives

```bash
# Show safe (removable) drives
engraver list

# Show all drives including system drives
engraver list --all

# Output as JSON
engraver list --json
```

### Write an Image

```bash
# Basic usage
engraver write ubuntu.iso /dev/sdb

# Write with verification
engraver write ubuntu.iso /dev/sdb --verify

# Skip confirmation prompt
engraver write ubuntu.iso /dev/sdb --yes

# Write from URL
engraver write https://releases.ubuntu.com/24.04/ubuntu.iso /dev/sdb

# Custom block size
engraver write ubuntu.iso /dev/sdb --block-size 1M

# Show partition layout before writing
engraver write ubuntu.iso /dev/sdb --show-partitions
engraver write ubuntu.iso /dev/sdb -p

# Enable checkpointing for resume support
engraver write ubuntu.iso /dev/sdb --checkpoint

# Resume an interrupted write
engraver write ubuntu.iso /dev/sdb --resume

# Verify checksum before writing
engraver write ubuntu.iso /dev/sdb --checksum abc123... --checksum-algo sha256

# Auto-detect checksum from companion files (.sha256, .sha512, .md5, SHA256SUMS, etc.)
engraver write ubuntu.iso /dev/sdb --auto-checksum
```

### Verify a Drive

```bash
engraver verify ubuntu.iso /dev/sdb
```

### Calculate Checksum

```bash
# SHA-256 (default)
engraver checksum ubuntu.iso

# MD5
engraver checksum ubuntu.iso --algorithm md5

# SHA-512
engraver checksum ubuntu.iso --algorithm sha512

# CRC32
engraver checksum ubuntu.iso --algorithm crc32
```

### Benchmark Drive Performance

```bash
# Basic benchmark (256 MB test, 4 MB blocks)
engraver benchmark /dev/sdb

# Custom test size and pattern
engraver benchmark /dev/sdb --size 1G --pattern random

# Test multiple block sizes to find optimal performance
engraver benchmark /dev/sdb --test-block-sizes "4K,64K,1M,4M,16M"

# Multiple passes
engraver benchmark /dev/sdb --passes 3

# JSON output for scripting
engraver benchmark /dev/sdb --json

# Skip confirmation
engraver benchmark /dev/sdb -y
```

**Benchmark options:**

| Option | Description | Default |
|--------|-------------|---------|
| `--size` | Amount of data to write | `256M` |
| `--block-size` | Block size for writes | `4M` |
| `--pattern` | Data pattern: `zeros`, `random`, `sequential` | `zeros` |
| `--passes` | Number of benchmark passes | `1` |
| `--test-block-sizes` | Test multiple block sizes (comma-separated) | - |
| `--json` | Output results as JSON | false |

**Warning:** Benchmarking is a destructive operation that will overwrite data on the target device.

### Manage Configuration

```bash
# View current settings
engraver config

# View settings as JSON
engraver config --json

# Show config file path
engraver config --path

# Create a default config file
engraver config --init
```

### Generate Shell Completions

```bash
engraver completions bash
engraver completions zsh
engraver completions fish
engraver completions powershell
engraver completions elvish
```

### Generate Man Pages

```bash
engraver mangen --out-dir ./man
```

## Configuration File

Engraver supports a configuration file at `~/.config/engraver/engraver_config.toml` (Linux/macOS) or `%APPDATA%\engraver\engraver_config.toml` (Windows).

```toml
[write]
block_size = "4M"     # Default block size
verify = true         # Always verify writes
checkpoint = false    # Enable checkpointing

[checksum]
algorithm = "sha256"  # Default checksum algorithm
auto_detect = false   # Auto-detect .sha256/.md5 files

[behavior]
skip_confirmation = false
quiet = false
```

Command-line arguments override configuration file settings.

## Platform-Specific Notes

### Linux

- Requires root privileges (use `sudo`)
- Device paths: `/dev/sdb`, `/dev/sdc`, etc.
- Uses O_DIRECT for bypassing page cache

### macOS

- Requires root privileges (use `sudo`)
- Device paths: `/dev/disk2`, `/dev/disk3`, etc.
- Automatically uses raw device paths (`/dev/rdisk2`) for better performance
- Uses F_NOCACHE for direct I/O

### Windows

- Requires Administrator privileges
- Device paths: `\\.\PhysicalDrive1`, `\\.\PhysicalDrive2`, etc.
- Uses FILE_FLAG_NO_BUFFERING for direct I/O

## Architecture

The CLI uses the `engraver-platform` crate for platform-specific device access:

```
engraver (CLI)
├── engraver-core      (writer, verifier, source handling, benchmark, resume)
├── engraver-detect    (drive detection, safety checks, USB speed)
└── engraver-platform  (raw device I/O, unmounting, sync)
```

This architecture ensures:
- Consistent behavior across platforms
- Optimal I/O performance via direct/unbuffered access
- Proper device handling (unmounting, syncing)

## Safety Features

Engraver includes multiple safety features to prevent accidental data loss:

1. **System drive detection**: Refuses to write to drives containing system partitions
2. **Confirmation prompt**: Requires explicit confirmation before writing
3. **Mount point display**: Shows what will be unmounted
4. **Size validation**: Checks that source fits on target
5. **USB speed warnings**: Alerts if USB 3.0 device runs at USB 2.0 speed

To bypass safety checks (DANGEROUS!):
```bash
engraver write image.iso /dev/sda --force --yes
```

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 130 | Interrupted (Ctrl+C) |

## Environment Variables

| Variable | Description |
|----------|-------------|
| `RUST_BACKTRACE` | Show error backtraces |
| `RUST_LOG` | Set log level (trace, debug, info, warn, error) |

## Shell Completions

Engraver can generate shell completions for various shells.

### Bash

```bash
# Generate and install completions
engraver completions bash > ~/.local/share/bash-completion/completions/engraver

# Or system-wide (requires sudo)
sudo engraver completions bash > /etc/bash_completion.d/engraver
```

### Zsh

```bash
# Generate completions
engraver completions zsh > ~/.zfunc/_engraver

# Add to .zshrc if not already present
echo 'fpath=(~/.zfunc $fpath)' >> ~/.zshrc
echo 'autoload -Uz compinit && compinit' >> ~/.zshrc
```

### Fish

```bash
engraver completions fish > ~/.config/fish/completions/engraver.fish
```

### PowerShell

```powershell
# Add to your PowerShell profile
engraver completions powershell >> $PROFILE
```

### Elvish

```bash
engraver completions elvish > ~/.elvish/lib/engraver.elv
```

## Man Pages

Generate and install man pages:

```bash
# Generate man pages to a directory
engraver mangen --out-dir ./man

# Install system-wide (requires sudo)
sudo cp ./man/*.1 /usr/local/share/man/man1/

# View man page
man engraver
man engraver-write
man engraver-verify
man engraver-list
man engraver-checksum
man engraver-benchmark
```

Generated man pages:
- `engraver.1` - Main command overview
- `engraver-write.1` - Write command documentation
- `engraver-verify.1` - Verify command documentation
- `engraver-list.1` - List command documentation
- `engraver-checksum.1` - Checksum command documentation
- `engraver-benchmark.1` - Benchmark command documentation

## Examples

### Write Ubuntu ISO to USB

```bash
# Download and write in one step
engraver write https://releases.ubuntu.com/24.04/ubuntu-24.04-desktop-amd64.iso /dev/sdb --verify

# Or write a local file
engraver write ~/Downloads/ubuntu.iso /dev/sdb --verify
```

### Write Compressed Image

```bash
# Engraver automatically detects and decompresses
engraver write raspbian.img.xz /dev/sdb
engraver write archlinux.iso.gz /dev/sdb
```

### Resume Interrupted Write

```bash
# Start with checkpointing
engraver write large-image.iso /dev/sdb --checkpoint

# If interrupted (Ctrl+C), resume later
engraver write large-image.iso /dev/sdb --resume
```

### Verify Existing Write

```bash
engraver verify ubuntu.iso /dev/sdb
```

## License

MIT License - see LICENSE file for details.
