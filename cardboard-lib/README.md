# cardboard-lib

A platform-agnostic embedded Rust library providing core device logic for Cardboard keyboard controllers. Designed for `no_std` environments with async support via Embassy.

## Overview

cardboard-lib provides the foundational abstractions and implementations for:

- **Keyboard profiles** - Layer-based key mappings with macro support
- **Key matrix scanning** - Debounced input handling for physical keys
- **Command handling** - Device operations via async command pattern
- **HID support** - N-Key Rollover keyboard, mouse, and consumer control
- **Storage abstractions** - Flash memory partitioning and profile persistence
- **Serial protocol** - Communication with host software

## Modules

| Module | Description |
|--------|-------------|
| `command` | Async command trait and implementations (Identify, UpdateProfile, GetProfile, etc.) |
| `context` | Runtime context holding flash, serial I/O, signals, and allocator |
| `device` | Device identification types (DeviceId, DeviceTypeId, CommandId) using UUIDs |
| `profile` | Keyboard profile structures (layers, keys, macros, virtual keys) |
| `state` | Keyboard state machine managing physical/virtual keys and macro execution |
| `hid` | HID abstractions for NKRO keyboard, mouse, and consumer control |
| `input` | Key matrix scanning with debouncing |
| `storage` | Flash memory traits and partition management |
| `serial` | Serial packet reader/writer abstractions |
| `embassy` | Embassy runtime integration (flash, serial, HID, clock implementations) |
| `error` | Lock-free error logging for `no_std` environments |
| `tasks` | Core async tasks for keypad scanning and command processing |

## Features

- **`embassy`** (default) - Enables Embassy async runtime support

## Building

```bash
# Debug build
cargo build

# Release build with optimizations
cargo build --release

# Run tests
cargo test
```

## Usage

This library is intended to be used as a dependency in firmware projects. Add it to your `Cargo.toml`:

```toml
[dependencies]
cardboard-lib = { path = "../cardboard-lib" }
```

### Context Setup

The library uses a generic `Context` struct that holds all runtime dependencies:

```rust
use cardboard_lib::{Context, TrackingAllocator};

// Create context with your platform-specific implementations
let context = Context::new(
    flash_memory,
    serial_reader,
    serial_writer,
    signals,
    allocator,
    error_log,
    clock,
);
```

## Key Concepts

### Strongly Typed IDs

All identifiers use UUID-based strongly typed wrappers to prevent mixing:

- `DeviceId` - Unique device identifier
- `LayerId` - Profile layer identifier
- `CommandId` - Command identifier
- `MacroId` - Macro identifier
- `KeyId` - Physical key identifier

### Profile Structure

Profiles define keyboard behavior with support for:

- Multiple layers
- Virtual keys (up to 32 per device)
- Macros with start, loop, and end sequences
- Layer switching based on tags
