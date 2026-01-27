# SAide

[![Rust](https://img.shields.io/badge/rust-1.85+-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

> **S**crcpy companion **A**pp with key/mouse mapp**i**ng - **A**n**de**

**SAide** is a high-performance Android device mirroring and control application written in Rust. It provides low-latency video streaming, audio capture, and customizable keyboard/mouse mapping for Android devices connected via USB or Wi-Fi.

[中文文档](README.zh.md) | [Documentation](docs/) | [Configuration Guide](docs/configuration.md)

---

## Features

### Core Capabilities

- 🚀 **Low-Latency Streaming**: 20-35ms end-to-end latency with Phase 3 optimizations
  - Hardware-accelerated video decoding (VAAPI/D3D11VA, NVDEC, software H.264 fallback)
  - Cross-platform: Linux (VAAPI), Windows (D3D11VA), all platforms (NVDEC/Software)
  - Optimized audio pipeline (64-frame buffer, configurable ring buffer)
  - TCP_QUICKACK and network optimizations
- 🎮 **Advanced Input Mapping**: Customizable keyboard and mouse mappings
  - Per-key touch coordinate mapping with rotation support
  - Drag detection, long-press recognition, adaptive thresholds
  - Toggle mappings on/off with F10 (configurable)
- 🎵 **Audio Streaming**: Real-time audio capture from Android 11+ devices
  - Opus/AAC codec support with configurable latency (1.3-5.3ms @ 48kHz)
  - Lock-free ring buffer for glitch-free playback
- 🖥️ **Modern UI**: Cross-platform desktop interface built with egui
  - Real-time FPS and latency indicators
  - Visual mapping editor (planned)
  - Settings panel (planned)
- 🌐 **Internationalization**: Full support for Chinese and English locales
  - Automatic language detection (system `$LANG`)
  - Hot-reload in debug mode

### Technical Highlights

- **Zero-copy GPU rendering** via wgpu (Vulkan/DirectX 12 backend)
- **Hardware acceleration**: 
  - Linux: VAAPI (Intel/AMD), NVDEC (NVIDIA)
  - Windows: D3D11VA (Intel/AMD/NVIDIA), NVDEC (NVIDIA)
  - All platforms: Software H.264 fallback
- **Robust error handling**: No panics in production code, comprehensive diagnostics
- **Configurable everything**: TOML-based configuration with validation and hot-reload
- **Profiling built-in**: 5-stage latency profiling (network → decode → upload → display)

---

## Quick Start

### Prerequisites

#### System Requirements

- **Operating System**: Linux (tested), Windows (experimental - v0.3), macOS (planned - v0.3)
- **Rust**: 1.85 or later
- **Android**: Device with Android 5.0+ (Android 11+ for audio)
- **ADB**: Android Debug Bridge installed and in PATH

#### Linux Dependencies

Install FFmpeg development libraries and graphics drivers:

```bash
# Debian/Ubuntu
sudo apt install libavcodec-dev libavformat-dev libavutil-dev libswscale-dev \
                 libopus-dev libasound2-dev pkg-config

# Arch Linux
sudo pacman -S ffmpeg opus alsa-lib

# Fedora/RHEL
sudo dnf install ffmpeg-devel opus-devel alsa-lib-devel
```

**Hardware acceleration** (optional but recommended):

```bash
# Intel/AMD (VAAPI)
sudo apt install libva-dev mesa-va-drivers

# NVIDIA (NVDEC) - requires proprietary drivers
# Install from: https://www.nvidia.com/Download/index.aspx
```

### Installation

#### 1. Clone the Repository

```bash
git clone https://github.com/yourusername/saide.git
cd saide
```

#### 2. Build from Source

```bash
# Debug build (fast compilation, unoptimized)
cargo build

# Release build (optimized, 3x faster runtime)
cargo build --release
```

#### 3. Run SAide

```bash
# Using cargo (debug)
cargo run

# Or directly (release)
./target/release/saide
```

---

## Usage

### Basic Workflow

1. **Connect your Android device** via USB (enable USB debugging in Developer Options)
2. **Launch SAide**:
   ```bash
   cargo run --release
   ```
3. **Authorize ADB** on your device when prompted
4. SAide will automatically:
   - Deploy scrcpy-server to the device
   - Establish video/audio/control streams
   - Start mirroring in the application window

### Keyboard Mapping

Create or edit `~/.config/saide/config.toml`:

```toml
[[mappings.key]]
key = "W"              # PC key to press
android_key = "UP"     # Android key to send (optional)
touch_x = 0.5          # Touch x-coordinate (0.0-1.0, normalized)
touch_y = 0.3          # Touch y-coordinate (0.0-1.0, normalized)
action = "both"        # "down", "up", or "both"
```

**Example**: WASD movement for mobile games:

```toml
[mappings]
toggle = "F10"         # Press F10 to enable/disable mappings
initial_state = true

[[mappings.key]]
key = "W"
touch_x = 0.5
touch_y = 0.2

[[mappings.key]]
key = "S"
touch_x = 0.5
touch_y = 0.8

[[mappings.key]]
key = "A"
touch_x = 0.2
touch_y = 0.5

[[mappings.key]]
key = "D"
touch_x = 0.8
touch_y = 0.5
```

### Audio Configuration

Reduce audio latency by adjusting buffer sizes:

```toml
[scrcpy.audio]
enabled = true
buffer_frames = 64       # Lower = less latency (64 ≈ 1.3ms @ 48kHz)
                         # Higher = fewer glitches (128/256)
ring_capacity = 5760     # Increase if experiencing audio dropouts
```

**Troubleshooting**:

- **Audio crackling/dropouts**: Increase `buffer_frames` to 128 or 256
- **High latency**: Decrease `buffer_frames` to 32 or 64 (may cause underruns on weak hardware)

See [Configuration Guide](docs/configuration.md) for full options.

---

## Configuration

SAide uses a TOML configuration file located at:

- **Linux**: `~/.config/saide/config.toml`
- **macOS**: `~/Library/Application Support/saide/config.toml`
- **Windows**: `%APPDATA%\saide\config.toml`

### Key Configuration Sections

| Section          | Purpose                                | Documentation                                                                      |
| ---------------- | -------------------------------------- | ---------------------------------------------------------------------------------- |
| `[general]`      | Window size, timeouts, bind address    | [docs/configuration.md](docs/configuration.md#general---general-settings)          |
| `[scrcpy.video]` | Bitrate, FPS, resolution, codec        | [docs/configuration.md](docs/configuration.md#scrcpyvideo---video-stream-settings) |
| `[scrcpy.audio]` | Buffer sizes, ring capacity, codec     | [docs/configuration.md](docs/configuration.md#scrcpyaudio---audio-stream-settings) |
| `[gpu]`          | Backend (Vulkan/OpenGL), VSync         | [docs/configuration.md](docs/configuration.md#gpu---gpu-rendering)                 |
| `[input]`        | Long-press, drag thresholds, intervals | [docs/configuration.md](docs/configuration.md#input---input-control-settings)      |
| `[mappings]`     | Keyboard/mouse mappings                | [docs/configuration.md](docs/configuration.md#mappings---keyboard-mapping)         |

**Example `config.toml`**: [config.toml](config.toml)

---

## Development

### Project Structure

```
saide/
├── src/
│   ├── main.rs              # Application entry point
│   ├── lib.rs               # Library exports
│   ├── saide/               # UI layer (egui)
│   │   ├── ui/              # SAideApp, Toolbar, Indicator
│   │   ├── init.rs          # Connection initialization
│   │   └── ...
│   ├── controller/          # Input handling
│   │   ├── keyboard.rs      # Keyboard mapper
│   │   ├── mouse.rs         # Mouse mapper
│   │   └── adb.rs           # ADB shell wrapper
│   ├── scrcpy/              # Scrcpy protocol
│   │   ├── protocol/        # Video/Audio/Control packets
│   │   ├── connection.rs    # TCP stream manager
│   │   └── server.rs        # scrcpy-server deployment
│   ├── decoder/             # Video/Audio decoding
│   │   ├── h264.rs          # Software H.264 decoder
│   │   ├── nvdec.rs         # NVIDIA hardware decoder (cross-platform)
│   │   ├── vaapi.rs         # Linux VAAPI hardware decoder
│   │   ├── d3d11va.rs       # Windows D3D11VA hardware decoder
│   │   └── audio/           # Opus/AAC audio decoding
│   ├── config/              # Configuration management
│   ├── i18n/                # Internationalization
│   └── profiler/            # Latency profiling
├── docs/                    # Documentation
│   ├── ARCHITECTURE.md      # System architecture
│   ├── configuration.md     # Configuration guide
│   ├── LATENCY_OPTIMIZATION.md  # Performance tuning
│   └── pitfalls.md          # Known issues & solutions
├── examples/                # Example programs
└── config.toml              # Default configuration
```

### Running Tests

```bash
# Run all tests
cargo test

# Run with verbose output
cargo test -- --nocapture

# Run specific test
cargo test test_audio_decode
```

### Code Quality Checks

```bash
# Format code
cargo fmt --all

# Lint with Clippy (strict mode)
cargo clippy -- -D warnings

# Check formatting + linting
cargo fmt --all -- --check && cargo clippy -- -D warnings
```

### Examples

SAide includes several standalone examples for testing components:

```bash
# Test scrcpy connection (no UI)
cargo run --example test_connection

# Test audio decoding and playback
cargo run --example test_audio

# Audio diagnostics (latency measurement)
cargo run --example audio_diagnostic

# AV sync testing with statistics
cargo run --example render_avsync
```

See [examples/](examples/) for full list.

---

## Performance

### Latency Breakdown (Phase 3 Optimizations)

| Stage            | Before  | After       | Optimization                          |
| ---------------- | ------- | ----------- | ------------------------------------- |
| **Network**      | 15-25ms | 10-15ms     | TCP_QUICKACK, remove flush            |
| **Decoding**     | 10-15ms | 8-12ms      | FFmpeg flags (FAST + EXPERIMENTAL)    |
| **Audio Buffer** | 2.7ms   | 1.3ms       | 128→64 frames @ 48kHz                 |
| **GPU Upload**   | 8-12ms  | 8-12ms      | (Phase 2 deferred - wgpu limitations) |
| **Display**      | 5-10ms  | 5-10ms      | VSync disabled by default             |
| **Total**        | 50-70ms | **20-35ms** | ✅ Target achieved                    |

**Profiling**: Built-in latency profiler tracks all 5 stages with P50/P95 statistics. Enable with:

```toml
[logging]
level = "debug"  # Shows per-frame latency stats
```

See [docs/LATENCY_OPTIMIZATION.md](docs/LATENCY_OPTIMIZATION.md) for detailed analysis.

---

## Roadmap

### Completed (v0.1)

- ✅ Basic video/audio streaming
- ✅ Hardware-accelerated decoding (VAAPI, NVDEC)
- ✅ Keyboard/mouse mapping with rotation support
- ✅ Configuration system with validation
- ✅ Internationalization (zh_CN, en_US)
- ✅ Latency optimizations (Phase 1 + 3.1)

### Planned

#### Near-term (v0.2 - Q1 2026)

- [ ] Visual mapping editor (drag-and-drop UI)
- [ ] Settings panel (GPU backend, codec, audio tuning)
- [ ] Log viewer (integrated tracing-appender)
- [ ] Clipboard synchronization (Android ↔ PC)

#### Mid-term (v0.3 - Q2 2026)

- [x] Windows hardware decoding (D3D11VA) - **Completed 2026-01-23**
- [ ] Windows GPU detection (DXGI enumeration)
- [ ] macOS support (VideoToolbox decoder)
- [ ] H.265/AV1 codec support
- [ ] File transfer (drag-and-drop files to device)
- [ ] Recording mode (save video/audio to file)

#### Long-term (v1.0 - 2026+)

- [ ] Wi-Fi connection without USB
- [ ] Multiple device support (simultaneous mirroring)
- [ ] Plugin system for custom input mappings
- [ ] Scripting API (automate device control)

See [TODO.md](TODO.md) for detailed task breakdown.

---

## Troubleshooting

### Common Issues

#### 1. **"ADB not found in PATH"**

**Solution**: Install Android SDK Platform-Tools:

```bash
# Debian/Ubuntu
sudo apt install android-tools-adb

# macOS
brew install android-platform-tools

# Or download from: https://developer.android.com/tools/releases/platform-tools
```

#### 2. **"Device unauthorized"**

**Solution**: Check your Android device screen for the "Allow USB debugging" prompt and authorize the computer.

#### 3. **Black screen / no video**

**Causes**:

- Device screen is off (check `turn_screen_off` in config)
- Codec mismatch (device doesn't support H.264)
- FFmpeg not installed

**Solution**:

```toml
[scrcpy.options]
turn_screen_off = false  # Keep device screen on
```

Check device supported codecs:

```bash
cargo run --example probe_codec
```

#### 4. **Audio crackling / dropouts**

**Solution**: Increase audio buffer size:

```toml
[scrcpy.audio]
buffer_frames = 128      # Or 256 for weak hardware
ring_capacity = 11520    # Double the default
```

#### 5. **High CPU usage**

**Causes**:

- Software decoding (no GPU acceleration)
- High FPS/resolution

**Solution**:

```toml
[scrcpy.video]
max_fps = 30            # Lower from 60
max_size = 1280         # Lower from 1920

[gpu]
backend = "VULKAN"      # Ensure hardware acceleration
```

Check GPU detection:

```bash
cargo run 2>&1 | grep -E "(Detected GPU|Using.*decoder)"
# Linux: "Detected GPU type: Intel" → "Using VAAPI hardware decoder"
# Windows: "Detected GPU type: Intel" → "Using D3D11VA hardware decoder"
# NVIDIA: "Using NVIDIA NVDEC hardware decoder"
```

#### 6. **Input lag / sluggish controls**

**Solution**: Reduce input thresholds:

```toml
[input]
long_press_ms = 200      # Faster long-press detection
drag_threshold_px = 3.0  # More sensitive drag
drag_interval_ms = 4     # Higher update rate (240fps)
```

### Debug Mode

Enable detailed logging:

```bash
RUST_LOG=debug cargo run
```

Or in `config.toml`:

```toml
[logging]
level = "debug"
```

### Known Issues

#### Windows-Specific (v0.3 - Experimental)

- **GPU detection returns "Unknown"**: D3D11VA still works, but decoder selection is not optimized. DXGI enumeration pending.
- **First run may be slow**: Windows Defender/antivirus may scan the executable on first launch.
- **Config path**: Use `%APPDATA%\saide\config.toml` instead of `~/.config/saide/config.toml`.
- **Connection drops during resolution changes (2026-01-27)**: FIXED in v0.3.1
  - **Symptom**: Video stream disconnects ~2.5 seconds after device rotation
  - **Root cause**: TOCTTOU race condition - `is_full()` checked after `try_send()` failed, but UI thread consumed frame between the two calls, causing false "disconnected" detection
  - **Fix**: Match `TrySendError::{Full, Disconnected}` directly instead of post-hoc `is_full()` check
  - **Impact**: Eliminates false disconnection on both Windows and Linux. Windows triggers more frequently due to slower overall performance (longer time in buffer Full state = larger race window), but the bug is platform-agnostic.
- **AMD GPU D3D11VA compatibility (2026-01-27)**:
  - Some AMD GPU/driver combinations may fail D3D11VA initialization with `Failed setup for format d3d11: hwaccel initialisation returned error`
  - **Workaround**: Update AMD GPU drivers to latest version from [AMD Support](https://www.amd.com/en/support)
  - **Testing**: Run `.\scripts\test_d3d11va_amd.ps1` to diagnose compatibility
  - **Fallback**: Set `hwdecode = false` in `config.toml` to force software decoding
  - **Root cause**: FFmpeg D3D11VA requires driver-level H.264 decode support (UVD/VCN). Older drivers (pre-2020) or APU integrated graphics may lack full support.

See [docs/pitfalls.md](docs/pitfalls.md) for comprehensive list of known issues and workarounds.

---

## Contributing

Contributions are welcome! Please follow these guidelines:

1. **Code Style**: Run `cargo fmt` before committing
2. **Linting**: Ensure `cargo clippy -- -D warnings` passes
3. **Tests**: Add tests for new features
4. **Documentation**: Update relevant docs (README, config.md, architecture.md)
5. **Commit Messages**: Use conventional commits (e.g., `feat: add H.265 support`)

### Development Workflow

```bash
# 1. Create feature branch
git checkout -b feature/my-feature

# 2. Make changes
# ... edit code ...

# 3. Check code quality
cargo fmt --all
cargo clippy -- -D warnings
cargo test

# 4. Commit
git add .
git commit -m "feat: describe your changes"

# 5. Push and create PR
git push origin feature/my-feature
```

---

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

## Acknowledgments

- **[scrcpy](https://github.com/Genymobile/scrcpy)**: Inspiration and protocol reference
- **[FFmpeg](https://ffmpeg.org/)**: Video/audio decoding
- **[egui](https://github.com/emilk/egui)**: Immediate mode GUI framework
- **[wgpu](https://github.com/gfx-rs/wgpu)**: Cross-platform GPU API

---

## Contact

- **Issues**: [GitHub Issues](https://github.com/yourusername/saide/issues)
- **Discussions**: [GitHub Discussions](https://github.com/yourusername/saide/discussions)

---

**Made with ❤️ in Rust**
