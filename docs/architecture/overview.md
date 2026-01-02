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
├── engraver-core      # Shared library (source, writer, verifier)
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

### engraver-platform

- Raw device I/O
- Unmounting filesystems
- Privilege checking
- Platform-specific operations

### engraver-core

- Source handling (local, remote, compressed)
- Block writing with progress
- Verification and checksums
- Configuration management

### engraver-cli

- Argument parsing
- User interaction
- Progress display
- JSON output for scripting

## Data Flow

```
Source (ISO/IMG/URL)
       │
       ▼
  ┌─────────┐
  │ Source  │ Decompress if needed
  │ Handler │
  └────┬────┘
       │
       ▼
  ┌─────────┐     ┌──────────┐
  │ Writer  │────▶│ Platform │ Raw device I/O
  │ Engine  │     │ Adapter  │
  └────┬────┘     └──────────┘
       │
       ▼
  ┌─────────┐
  │Verifier │ Checksum comparison
  └─────────┘
```
