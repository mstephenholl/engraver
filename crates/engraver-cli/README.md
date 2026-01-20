# Engraver CLI

A safe, fast command-line tool for creating bootable USB drives.

## Features

- **Safe by default**: Refuses to write to system drives
- **Multiple sources**: Local files, URLs, compressed archives
- **Verification**: Optional post-write verification
- **Progress display**: Real-time progress with speed and ETA
- **Cross-platform**: Linux, macOS, Windows support
- **Direct I/O**: Bypasses page cache for optimal write performance
- **Platform-native**: Uses platform-specific APIs for device access

## Installation

```bash
cargo install engraver
```

Or build from source:

```bash
cd crates/engraver-cli
cargo build --release
```

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

# Verify checksum before writing
engraver write ubuntu.iso /dev/sdb --checksum abc123... --checksum-algo sha256
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
```

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
├── engraver-core      (writer, verifier, source handling)
├── engraver-detect    (drive detection, safety checks)
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
```

Generated man pages:
- `engraver.1` - Main command overview
- `engraver-write.1` - Write command documentation
- `engraver-verify.1` - Verify command documentation  
- `engraver-list.1` - List command documentation
- `engraver-checksum.1` - Checksum command documentation

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

### Verify Existing Write

```bash
engraver verify ubuntu.iso /dev/sdb
```

## License

MIT License - see LICENSE file for details.
