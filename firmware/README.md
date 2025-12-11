# Cardboard Firmware

Embedded firmware for RP2040-based Cardboard keyboard controllers. Built with Embassy async runtime and the `cardboard-lib` core library.

## Overview

This firmware provides a complete keyboard controller implementation featuring:

- **USB HID** - N-Key Rollover keyboard, mouse, and consumer control
- **USB Serial** - CDC-ACM interface for host communication
- **Profile storage** - Persistent keyboard profiles in flash memory
- **Macro support** - Programmable key sequences
- **Layer switching** - Dynamic key mappings via tags

## Hardware Support

### CK1-30 (Primary Target)

- **MCU**: RP2040 (Raspberry Pi Pico)
- **Keys**: 30-key matrix (5 rows × 6 columns) with up to 32 virtual keys
- **Flash**: 2 MB (500 KB allocated for profiles/settings)
- **Heap**: 96 KB

**Pin Configuration**:
- Row pins (output): GPIO 28, 27, 26, 22, 21
- Column pins (input): GPIO 16, 17, 9, 18, 19, 20

## Building

### Prerequisites

- Rust nightly toolchain
- `thumbv6m-none-eabi` target
- `probe-rs` for flashing (optional)

```bash
# Install target
rustup target add thumbv6m-none-eabi

# Install probe-rs for flashing
cargo install probe-rs-tools
```

### Build Commands

```bash
# Debug build
cargo build

# Release build
cargo build --release

# Flash debug firmware
cargo run

# Flash release firmware
cargo run --release
```

The default runner will use `elf2uf2-rs` to flash the firmware to a Raspberry Pi Pico connected to the system in bootloader mode. To use `probe-rs`, edit `.cargo/config.toml`, comment out the `elf2uf2-rs` runner, and uncomment the `probe-rs` runner.

## Memory Layout

| Region | Offset | Size | Purpose |
|--------|--------|------|---------|
| Settings | 0x0 | 4 KB | Device settings |
| Profiles | 0x1000 | 496 KB | Keyboard profiles |

Total flash allocation: 500 KB at end of 2 MB flash.

## Architecture

### Task Model

The firmware runs multiple concurrent tasks on the Embassy executor:

1. **keypad_task** - Scans key matrix, manages keyboard state, executes macros, generates HID reports
2. **cmd_task** - Processes serial commands from host software
3. **hid_task** - Distributes HID reports to USB endpoints
4. **usb_task** - Main USB device loop

### Inter-task Communication

Tasks communicate via lock-free Embassy signals:

- `HID_SIGNAL` - HID report distribution
- `PROFILE_CHANGED_SIGNAL` - Profile update notifications
- `EXTERNAL_TAGS_CHANGED_SIGNAL` - Layer tag changes
- `VIRTUAL_KEY_SIGNAL` - Virtual key state updates

### USB Configuration

| Endpoint | Type | Packet Size |
|----------|------|-------------|
| Keyboard | HID | 32 bytes |
| Mouse | HID | 32 bytes |
| Consumer Control | HID | 32 bytes |
| Serial | CDC-ACM | 64 bytes |

## Project Structure

```
firmware/
├── src/
│   ├── lib.rs              # Library root, serial number helper
│   ├── ck1_30/
│   │   └── main.rs         # CK1-30 entry point and initialization
│   └── rp2040/
│       ├── mod.rs          # RP2040 module exports
│       ├── bootloader.rs   # Reboot and bootloader entry
│       ├── flash.rs        # Flash memory initialization
│       ├── hid.rs          # HID report task
│       └── usb.rs          # USB device setup
├── Cargo.toml              # Dependencies and build config
├── Embed.toml              # Debug probe configuration
├── build.rs                # Linker script setup
└── memory.x                # Memory layout definition
```

## Bootloader Entry

For convenience, the firmware supports entering the RP2040 USB bootloader for firmware updates. This is triggered via:
- Special key combination (KEY[0] at boot)
- Serial command from host software
