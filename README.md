# SAide

[![Rust](https://img.shields.io/badge/rust-1.85+-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)

SAide is a desktop client for mirroring and controlling Android devices through scrcpy — low-latency video, Android 11+ audio capture, and per-device key/mouse mapping in an egui app.

[中文说明](README.zh.md) · [Architecture](docs/ARCHITECTURE.md) · [Protocol Notes](docs/SCRCPY_PROTOCOL.md) · [Configuration Guide](docs/configuration.md)

## Features

- low-latency Android mirroring via `eframe + egui + wgpu`
- scrcpy video, audio, and control channel integration
- per-device and per-rotation mapping profiles
- hardware decoding: NVDEC, VAAPI, D3D11VA with software fallback
- built-in latency profiling and status indicators
- one-click screenshot and screen recording saved locally as PNG / MP4

## Anti-Detection System

SAide includes a behavior simulation engine that humanizes all input operations to reduce the risk of detection by game anti-cheat systems.

**What it does:**

- **Jittering** — randomizes touch coordinates (±0.5%–±5%) and multi-finger pinch spacing so clicks and swipes never hit the exact same pixel
- **Delay randomization** — inserts realistic Gaussian-distributed pauses between actions (20–500 ms) and within taps (TouchDown → TouchUp, 30–200 ms)
- **Bézier path smoothing** — replaces straight-line swipes with cubic-Bézier paths that mimic the natural curve of a human finger
- **Character-by-character typing** — sends text one character at a time with variable inter-key delays instead of pasting in one block
- **Touch pressure variation** — varies the `pressure` field (0.3–1.0) using a Gaussian distribution rather than a fixed value of 1.0
- **Pointer ID rotation** — cycles through multiple pointer IDs across operations so no single ID becomes a tracking fingerprint
- **Micro-tremor** — overlays 8–12 Hz physiological tremor onto move and long-press events (0.5–2 px amplitude)
- **Session rhythm** — introduces macro-level activity cycles (5–15 min) with idle pauses (2–10 s) that simulate natural attention decay
- **Rate limiting** — a token-bucket limiter prevents operation bursts that exceed human reaction speeds
- **Stall detection** — monitors video frames to detect and warn about suspiciously uniform input patterns

**Configuration:**

Enable anti-detection in `config.toml`:

```toml
[behavior]
preset = "balanced"   # conservative | balanced | aggressive
enabled = true
```

| Preset          | Jitter  | Delay (ms)   | Path smoothing | Typing | Pressure | Tremor |
| --------------- | ------- | ------------ | -------------- | ------ | -------- | ------ |
| `conservative`  | ±0.5%   | 0            | off            | off    | off      | off    |
| `balanced`      | ±3%     | 80 (20–200)  | on             | on     | on       | on     |
| `aggressive`    | ±5%     | 200 (50–500) | on             | on     | on       | on     |

Every option can be tuned individually — see [`config.behavior-example.toml`](config.behavior-example.toml) for the full reference. When the `[behavior]` section is absent, SAide falls back to `conservative` defaults with no extra delay.

## Quick start

1. Download the latest release for your platform from [Releases](https://github.com/keivry/saide/releases).
2. Enable USB debugging on your Android device and ensure `adb` is in `PATH`.
3. Run `saide`. Android 11+ required for audio capture.

<details>
<summary>Building from source</summary>

**Requirements:** Rust 1.85+, `adb` in `PATH`, USB debugging enabled.

### Linux

```bash
# Debian/Ubuntu
sudo apt install libavcodec-dev libavformat-dev libavutil-dev libswscale-dev \
                 libopus-dev libasound2-dev pkg-config

# Arch
sudo pacman -S ffmpeg opus alsa-lib

# Fedora/RHEL
sudo dnf install ffmpeg-devel opus-devel alsa-lib-devel
```

The project requires FFmpeg `8.x`. Verify with `pkg-config --modversion libavcodec`. If your distro ships an older version, build FFmpeg from source:

```bash
FFMPEG_VERSION=8.0.1
FFMPEG_PREFIX="$HOME/.local/ffmpeg-${FFMPEG_VERSION}"

curl -L "https://ffmpeg.org/releases/ffmpeg-${FFMPEG_VERSION}.tar.xz" | tar -xJ -C /tmp
cd "/tmp/ffmpeg-${FFMPEG_VERSION}"

./configure --prefix="$FFMPEG_PREFIX" --enable-gpl --enable-pic \
            --enable-shared --disable-static --disable-programs \
            --disable-doc --disable-debug
make -j"$(nproc)" && make install

export FFMPEG_DIR="$FFMPEG_PREFIX"
export PKG_CONFIG_PATH="$FFMPEG_PREFIX/lib/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
export LD_LIBRARY_PATH="$FFMPEG_PREFIX/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
```

VAAPI hardware decoding requires the appropriate GPU driver (Intel/AMD).

### Windows

Use the MSVC toolchain with FFmpeg and Opus from `vcpkg`. The repository includes `.github/vcpkg-triplets/x64-windows-release.cmake`.

```powershell
choco install -y pkgconfiglite

git clone https://github.com/microsoft/vcpkg "$env:USERPROFILE\vcpkg"
& "$env:USERPROFILE\vcpkg\bootstrap-vcpkg.bat" -disableMetrics

$env:VCPKG_ROOT = "$env:USERPROFILE\vcpkg"
$env:VCPKG_DEFAULT_TRIPLET = "x64-windows-release"
$env:VCPKG_TARGET_TRIPLET = "x64-windows-release"
$env:VCPKG_OVERLAY_TRIPLETS = "$PWD\.github\vcpkg-triplets"
$env:VCPKGRS_DYNAMIC = "1"

& "$env:VCPKG_ROOT\vcpkg.exe" install --overlay-triplets "$PWD\.github\vcpkg-triplets" "ffmpeg[nvcodec]:x64-windows-release" "opus:x64-windows-release"

$env:FFMPEG_DIR = "$env:VCPKG_ROOT\installed\x64-windows-release"
$env:PKG_CONFIG_PATH = "$env:VCPKG_ROOT\installed\x64-windows-release\lib\pkgconfig"
$env:Path = "$env:VCPKG_ROOT\installed\x64-windows-release\bin;C:\ProgramData\chocolatey\bin;" + $env:Path

cargo build --release --target x86_64-pc-windows-msvc
```

NVDEC requires the NVIDIA CUDA Toolkit with `CUDA_PATH` set. If you don't need it, replace `ffmpeg[nvcodec]` with `ffmpeg`.

### Run

```bash
git clone https://github.com/keivry/saide.git
cd saide
cargo run --release
```

</details>

## Configuration

Config file location (searched in order): platform config dir → `./config.toml` → created at standard path if absent. See [`config.toml`](config.toml) for a sample.

| Section            | Purpose                                                                       |
| ------------------ | ----------------------------------------------------------------------------- |
| `[general]`        | keyboard/mouse enable, toolbar, window size, bind address, scrcpy server path |
| `[scrcpy.video]`   | bitrate, fps, max size, codec, encoder                                        |
| `[scrcpy.audio]`   | audio enablement, codec, source, buffering                                    |
| `[scrcpy.options]` | screen-off and stay-awake                                                     |
| `[behavior]`       | anti-detection: touch, typing, and timing humanization                        |
| `[gpu]`            | backend (`VULKAN`/`OPENGL`), vsync, hardware decode                           |
| `[input]`          | long press, drag threshold, drag interval                                     |
| `[mappings]`       | toggle key and per-device profiles                                            |
| `[logging]`        | log level                                                                     |

Full reference: [docs/configuration.md](docs/configuration.md)

## Mapping example

```toml
[mappings]
toggle = "F10"
initial_state = false

[[mappings.profiles]]
name = "Portrait"
device_serial = "ABC123"
rotation = 0

[[mappings.profiles.mappings]]
key = "W"
action = "Tap"
x = 0.5
y = 0.3
```

## Troubleshooting

**`adb` not found** — install Android platform-tools and add `adb` to `PATH`.

**No audio** — requires Android 11 / API 30+. SAide falls back to video+control automatically on older devices.

**High latency** — try reducing `scrcpy.video.max_fps` / `max_size`, increasing `scrcpy.audio.buffer_frames` / `ring_capacity`, or disabling `gpu.vsync`.

**Theme** — set `SAIDE_THEME=dark|light|auto` to override the detected theme.

## For contributors

```bash
cargo fmt --all -- --check
cargo clippy -- -D warnings
cargo test --quiet
```

Examples: `test_connection`, `test_audio`, `audio_diagnostic`, `render_avsync`, `probe_codec`, `test_protocol`, `test_auto_decoder`, `test_audio_native`, `test_i18n`, `test_planar_interleave`, `test_vulkan_import`.

Conventional commit prefixes (`feat:`, `fix:`, `docs:`, `refactor:`). Keep docs and examples in sync with code changes.

## License

MIT OR Apache-2.0. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
