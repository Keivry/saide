# Development Guide

This guide covers setting up the development environment, building the project, and understanding the development workflow.

## Table of Contents

1. [Prerequisites](#1-prerequisites)
2. [Environment Setup](#2-environment-setup)
3. [Building](#3-building)
4. [Running](#4-running)
5. [Testing](#5-testing)
6. [Code Quality](#6-code-quality)
7. [Debugging](#7-debugging)
8. [Adding New Features](#8-adding-new-features)
9. [Troubleshooting](#9-troubleshooting)

---

## 1. Prerequisites

### System Requirements

- **OS**: Linux (Ubuntu 22.04+ recommended)
- **Memory**: 8GB+ RAM
- **Storage**: 2GB+ free space
- **GPU**: NVIDIA or Intel GPU for hardware acceleration (optional)

### Required Tools

| Tool | Minimum Version | Purpose |
|------|-----------------|---------|
| Rust | 1.70.0 | Compiler and build toolchain |
| cargo | 1.70.0 | Rust package manager |
| clang | 14.0 | C compiler for FFmpeg bindings |
| pkg-config | 0.29 | Library detection |
| FFmpeg | 5.0 | Media framework |
| ADB | 1.0.41 | Android debug bridge |

### Optional Tools

| Tool | Purpose |
|------|---------|
| LLDB/GDB | Debugging |
| cargo-nextest | Enhanced test runner |
| cargo-expand | Macro expansion |
| rust-analyzer | IDE support |

---

## 2. Environment Setup

### Step 1: Install Rust

```bash
# Install rustup (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Use stable toolchain
rustup default stable

# Verify installation
rustc --version
cargo --version
```

### Step 2: Install System Dependencies

#### Ubuntu/Debian

```bash
# Update package list
sudo apt update

# Install build tools
sudo apt install -y \
    build-essential \
    clang \
    pkg-config \
    git

# Install FFmpeg and development headers
sudo apt install -y \
    libavcodec-dev \
    libavformat-dev \
    libavutil-dev \
    libswscale-dev \
    libswresample-dev

# Install VAAPI development headers (Intel GPU)
sudo apt install -y libva-dev libva-drm2

# Install NVIDIA CUDA development headers (NVIDIA GPU)
# For Ubuntu, add CUDA repository first
# https://developer.nvidia.com/cuda-downloads
sudo apt install -y nvidia-cuda-toolkit
```

#### Arch Linux

```bash
# Install base development tools
sudo pacman -S --needed \
    base-devel \
    clang \
    pkgconf \
    git

# Install FFmpeg
sudo pacman -S \
    ffmpeg

# Install VAAPI (Intel)
sudo pacman -S libva libva-intel-driver

# Install NVIDIA CUDA
sudo pacman -S cuda
```

### Step 3: Install scrcpy-server

The scrcpy-server.jar must be placed in the project root:

```bash
# Download scrcpy-server
wget https://github.com/Genymobile/scrcpy/releases/download/v3.3.3/scrcpy-server-v3.3.3

# Rename to expected filename
mv scrcpy-server-v3.3.3 scrcpy-server.jar

# Verify
ls -la scrcpy-server.jar
```

### Step 4: Set Up Android Device

1. Enable developer options on Android device
2. Enable USB debugging
3. Connect device via USB
4. Verify ADB connection:

```bash
adb devices
# Should show: "device_serial    device"
```

---

## 3. Building

### Standard Build

```bash
# Clone the repository
git clone https://github.com/yourusername/saide.git
cd saide

# Build the project
cargo build
```

### Release Build

```bash
cargo build --release
```

### Hardware Acceleration Variants

#### NVIDIA GPU (NVDEC)

```bash
cargo build --release --features nvdec
```

#### Intel GPU (VAAPI)

```bash
cargo build --release --features vaapi
```

#### All Hardware Acceleration

```bash
cargo build --release --features "nvdec,vaapi"
```

### Software Decode Only

```bash
cargo build --release --no-default-features --features software_decode
```

### Verbose Build Output

```bash
cargo build -vv  # Shows compiler invocations
```

---

## 4. Running

### Basic Usage

```bash
# Run with default settings
cargo run

# Run with specific device serial
cargo run -- --serial <device_serial>

# Run in debug mode with logging
RUST_LOG=debug cargo run
```

### Command-Line Options

```bash
cargo run -- --help
```

Output:
```
SAide - Android Device Remote Control

Usage: saide [OPTIONS]

Options:
  -s, --serial <SERIAL>     Device serial number
  -m, --max-size <SIZE>     Maximum video dimension (default: 1920)
  -b, --bit-rate <RATE>     Video bit rate (default: 8M)
  -f, --max-fps <FPS>       Maximum frame rate (default: 60)
      --video-codec <CODEC> Video codec (h264, h265, av1)
      --audio               Enable audio streaming
      --no-audio            Disable audio streaming
      --stay-awake          Prevent device from sleeping
      --turn-screen-off     Turn off device screen on connect
  -h, --help                Print help information
  -V, --version             Print version information
```

### Environment Variables

| Variable | Values | Purpose |
|----------|--------|---------|
| `RUST_LOG` | `debug`, `info`, `warn`, `error` | Logging level |
| `RUST_BACKTRACE` | `1` | Enable backtraces on panic |
| `SAIDE_CONFIG` | Path | Custom config file path |

### Examples

```bash
# High quality streaming
cargo run -- --max-size 1080 --bit-rate 16M

# Audio enabled
cargo run -- --audio

# Low latency mode
cargo run -- --max-size 720 --bit-rate 4M --max-fps 30

# Quiet mode (errors only)
RUST_LOG=error cargo run
```

---

## 5. Testing

### Run All Tests

```bash
# Run unit tests
cargo test

# Run doc tests
cargo test --doc

# Run integration tests
cargo test --tests
```

### Enhanced Test Runner

```bash
# Install cargo-nextest (recommended)
cargo install cargo-nextest

# Run tests with nextest
cargo nextest run

# Generate coverage report
cargo nextest run --cov
```

### Test Categories

```bash
# Run only unit tests
cargo test lib

# Run only integration tests
cargo test tests

# Run specific test
cargo test test_name
```

### Protocol Tests

```bash
# Test protocol implementation
cargo test --package saide --lib scrcpy::protocol

# Test control message serialization
cargo test --package saide --lib controller::control_sender

# Test video packet parsing
cargo test --package saide --lib scrcpy::protocol::video
```

---

## 6. Code Quality

### Formatting

```bash
# Check formatting
cargo fmt --all -- --check

# Auto-format code
cargo fmt --all
```

### Linting

```bash
# Run clippy
cargo clippy --all-targets --all-features

# Clippy with strict warnings
cargo clippy --all-targets --all-features -- -D warnings
```

### Documentation

```bash
# Generate API documentation
cargo doc --no-deps --open

# Check for documentation errors
cargo doc --no-deps --document-private-items
```

### Unused Code Detection

```bash
# Check for unused dependencies
cargo +nightly udeps

# Check for dead code
cargo +nightly deadcode
```

### All Checks (Pre-commit)

```bash
# Run all quality checks
cargo fmt --all -- --check && \
cargo clippy --all-targets --all-features -- -D warnings && \
cargo test --quiet && \
cargo doc --no-deps
```

---

## 7. Debugging

### Logging

```bash
# Enable debug logging
RUST_LOG=debug cargo run 2>&1 | grep -E "(DEBUG|INFO|WARN|ERROR)"

# Log to file
RUST_LOG=debug cargo run > saide.log 2>&1

# Filter by module
RUST_LOG=debug,saide::scrcpy=trace,saide::decoder=trace cargo run
```

### Debugging with LLDB

```bash
# Build with debug symbols (default in debug mode)
cargo build

# Attach to process
lldb $(which saide)

# Or run with debugger
lldb -- cargo run
```

### Debugging with GDB

```bash
cargo build
gdb ./target/debug/saide
```

### Common Debug Commands

```rust
// Add debug output in code
log::debug!("Variable value: {:?}", variable);

// Log with format
log::info!("Frame {}: pts={}", frame_count, pts);

// Trace function calls
log::trace!("Entering function {}", function_name);
```

### Protocol Debugging

```bash
# Log protocol messages
RUST_LOG=debug,saide::scrcpy::protocol=trace cargo run

# Hexdump video packets
RUST_LOG=debug,saide::scrcpy::protocol::video=trace cargo run 2>&1 | xxd
```

### GPU Debugging

For NVDEC issues:
```bash
# Enable CUDA verbose logging
RUST_LOG=debug cargo run 2>&1 | grep -i cuda

# Check GPU info
nvidia-smi
```

For VAAPI issues:
```bash
# Check VAAPI support
vainfo

# Check Intel GPU
intel_gpu_top
```

---

## 8. Adding New Features

### Step 1: Design

1. Create design document in `docs/`
2. Update `TODO.md` with task
3. Get design review (if major feature)

### Step 2: Implement

1. Create new module or extend existing
2. Add unit tests
3. Update documentation

### Step 3: Test

1. Run existing tests
2. Add integration tests
3. Manual testing

### Step 4: Code Review

1. Format code: `cargo fmt --all`
2. Run linter: `cargo clippy --all-features`
3. Submit PR

### Example: Adding New Control Message

1. Define message type in `src/scrcpy/protocol/control.rs`:

```rust
pub enum ControlMessage {
    // Existing messages...
    NewFeature { param: u32 },
}

impl ControlMessage {
    pub fn serialize(&self, buf: &mut Vec<u8>) -> Result<usize> {
        match self {
            // Existing implementations...
            Self::NewFeature { param } => {
                buf.write_u8(TYPE_NEW_FEATURE)?;
                buf.write_u32::<BigEndian>(*param)?;
                Ok(5)
            }
        }
    }
}
```

2. Add test in `src/scrcpy/protocol/control.rs`:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_new_feature_serialization() {
        let msg = ControlMessage::new_feature(42);
        let mut buf = Vec::new();
        let size = msg.serialize(&mut buf).unwrap();
        assert_eq!(size, 5);
        assert_eq!(buf[0], TYPE_NEW_FEATURE);
    }
}
```

3. Update protocol documentation in `docs/SCRCPY_PROTOCOL.md`

### Example: Adding New Decoder

1. Create new decoder in `src/decoder/video/`:

```rust
pub struct MyDecoder {
    // Decoder state
}

impl VideoDecoder for MyDecoder {
    fn new(config: &DecoderConfig) -> Result<Self> {
        // Initialization
    }

    fn send_packet(&mut self, packet: &[u8]) -> Result<()> {
        // Decode packet
    }

    fn receive_frame(&mut self) -> Result<DecodedFrame> {
        // Get decoded frame
    }
}
```

2. Register decoder in `src/decoder/mod.rs`

3. Add tests and documentation

---

## 9. Troubleshooting

### Build Errors

#### Missing FFmpeg Headers

```
error: could not find native library: -lavcodec
```

Solution:
```bash
# Ubuntu/Debian
sudo apt install libavcodec-dev libavformat-dev libavutil-dev

# Verify installation
pkg-config --modversion libavcodec
```

#### Missing VAAPI Headers

```
error: failed to select a version for `libva-sys`
```

Solution:
```bash
# Ubuntu/Debian
sudo apt install libva-dev libva-drm2

# Arch Linux
sudo pacman -S libva libva-intel-driver
```

#### CUDA Not Found

```
error: could not find native library: -lcudart
```

Solution:
```bash
# Install CUDA toolkit
sudo apt install nvidia-cuda-toolkit

# Or disable CUDA features
cargo build --no-default-features
```

### Runtime Errors

#### ADB Device Not Found

```
Error: No device found
```

Solution:
1. Check USB debugging is enabled on device
2. Verify ADB connection: `adb devices`
3. Restart ADB server: `adb kill-server && adb start-server`

#### Scrcpy Server Connection Failed

```
Error: Failed to connect to scrcpy-server
```

Solution:
1. Verify scrcpy-server.jar is in project root
2. Check file permissions: `chmod 644 scrcpy-server.jar`
3. Check device compatibility (Android 5.0+ required)

#### Audio Not Working

```
Warn: Audio capture requires Android 11+
```

Solution:
1. Update device to Android 11+
2. Or run with `--no-audio`
3. Check device audio settings

#### Video Decode Error

```
Error: Decode failed
```

Solution:
1. Try software decode: `cargo run --no-default-features`
2. Check GPU drivers are up to date
3. Verify hardware acceleration is supported

#### Black Screen

```
Issue: Video not displayed after connection
```

Solution:
1. Check logs: `RUST_LOG=debug cargo run`
2. Verify video codec support
3. Try different resolution: `--max-size 720`

#### High Latency

```
Issue: Noticeable input lag
```

Solution:
1. Use hardware decode (VAAPI/NVDEC)
2. Reduce resolution: `--max-size 720`
3. Increase bit rate: `--bit-rate 16M`
4. Use USB connection instead of WiFi

### Performance Issues

#### High CPU Usage

```bash
# Check CPU usage
top -H -p $(pgrep saide)

# Try software decode fallback
cargo run --no-default-features
```

#### GPU Memory Issues

```bash
# Check GPU memory
nvidia-smi  # For NVIDIA
intel_gpu_top  # For Intel
```

### Getting Help

1. Check existing issues: `docs/pitfalls.md`
2. Search existing GitHub issues
3. Enable debug logging and capture output
4. Create new issue with:
   - OS version
   - Rust version
   - Full error output
   - Steps to reproduce

---

## Development Workflow

### Git Workflow

```bash
# Create feature branch
git checkout -b feature/new-feature

# Make changes
# ... edit code ...

# Run checks
cargo fmt --all
cargo clippy --all-features
cargo test

# Commit
git add .
git commit -m "feat: Add new feature description"

# Push
git push origin feature/new-feature
```

### Commit Message Format

```
<type>(<scope>): <subject>

<body>

<footer>
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `refactor`: Code refactoring
- `docs`: Documentation
- `test`: Adding tests
- `chore`: Maintenance

Example:
```
feat(controller): Add new control message type

Implement TYPE_NEW_FEATURE for advanced device control.

Closes #123
```

### Release Process

1. Update version in `Cargo.toml`
2. Update `CHANGELOG.md`
3. Create release tag
4. Build release binary
5. Create GitHub release

---

## Related Documentation

- [Architecture Overview](ARCHITECTURE.md) - System architecture
- [Protocol Specification](SCRCPY_PROTOCOL.md) - Scrcpy protocol details
- [Pitfalls & Lessons](pitfalls.md) - Known issues and solutions
- [Task Tracker](TODO.md) - Project progress tracking
