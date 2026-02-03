# Engraver Architecture Overview

## Design Principles

1. **Safety First** - Prevent accidental system drive overwrites
2. **Speed & Efficiency** - High-performance I/O with minimal overhead
3. **Reliability** - Post-write verification for data integrity
4. **Developer Experience** - Clean APIs, good error messages, scriptable
5. **Cross-Platform** - Consistent behavior on Linux, macOS, Windows
6. **Simplicity** - Do one thing well

## Crate Structure

```
engraver/
├── engraver-core      # Shared library (source, writer, verifier, benchmark, resume)
├── engraver-cli       # Command-line interface
├── engraver-gui       # Graphical interface (placeholder)
├── engraver-platform  # OS-specific adapters
└── engraver-detect    # Drive detection (safety-critical)
```

## Component Responsibilities

### engraver-detect (Safety-Critical)

- Enumerate available drives
- Identify system vs removable drives
- Prevent writes to system drives
- Detect USB connection speeds
- Parse partition information

### engraver-platform

- Raw device I/O (O_DIRECT, F_NOCACHE, FILE_FLAG_NO_BUFFERING)
- Unmounting filesystems
- Privilege checking
- Platform-specific operations

### engraver-core

- **Source handling**: Local files, remote URLs, compressed archives, cloud storage
- **Block writing**: High-performance writer with progress tracking
- **Verification**: Post-write checksums (SHA-256, SHA-512, MD5, CRC32)
- **Benchmark**: Drive performance testing with configurable patterns
- **Resume**: Checkpoint-based resumption of interrupted writes
- **Partition inspection**: MBR and GPT partition table parsing
- **Settings**: Persistent user configuration (TOML-based)

### engraver-cli

- Argument parsing (clap)
- User interaction (dialoguer)
- Progress display (indicatif)
- JSON output for scripting

### engraver-gui (Planned)

- Visual drive selection
- Progress monitoring
- Drag-and-drop ISO support

## Data Flow

```
Source (ISO/IMG/URL/Cloud)
       │
       ▼
  ┌─────────┐
  │ Source  │ Decompress if needed (gz, xz, zst, bz2)
  │ Handler │ Stream from HTTP/S3/GCS/Azure
  └────┬────┘
       │
       ▼
  ┌─────────┐     ┌──────────┐
  │ Writer  │────▶│ Platform │ Raw device I/O
  │ Engine  │     │ Adapter  │ (O_DIRECT, etc.)
  └────┬────┘     └──────────┘
       │
       │ (optional)
       ▼
  ┌─────────┐
  │Verifier │ Checksum comparison
  └─────────┘
```

## Resume/Checkpoint Flow

```
Write Start
    │
    ▼
┌───────────────┐
│ Create        │ Save: source info, target, position
│ Checkpoint    │
└───────┬───────┘
        │
        ▼
   Write Blocks ──────┐
        │             │ (periodic save)
        ▼             ▼
   Interrupted? ─Yes─▶ Save Checkpoint
        │
       No
        │
        ▼
   Complete ─────────▶ Remove Checkpoint
```

## Error Handling

The project uses typed errors throughout:

- `engraver_core::Error` - Core operation errors
- `engraver_detect::DetectError` - Drive enumeration errors
- `engraver_platform::PlatformError` - I/O and system errors

All errors include context and suggestions for resolution.
