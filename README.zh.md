# SAide

[![Rust](https://img.shields.io/badge/rust-1.85+-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)

SAide 是一个通过 scrcpy 镜像与控制 Android 设备的桌面客户端——低延迟视频、Android 11+ 音频采集，以及按设备划分的按键/鼠标映射，基于 egui 构建。

[English](README.md) · [架构说明](docs/ARCHITECTURE.md) · [协议说明](docs/SCRCPY_PROTOCOL.md) · [配置指南](docs/configuration.md)

## 特性

- 基于 `eframe + egui + wgpu` 的低延迟 Android 镜像
- 集成 scrcpy 的视频、音频和控制通道
- 按设备和屏幕旋转区分的映射配置
- 支持 NVDEC、VAAPI、D3D11VA 硬件解码，并带软件回退
- 内置延迟分析与状态指示

## 快速开始

1. 从 [Releases](https://github.com/keivry/saide/releases) 下载适合平台的最新版本。
2. 在 Android 设备上开启 USB 调试，确保 `adb` 已加入 `PATH`。
3. 运行 `saide`。音频采集需要 Android 11+。

<details>
<summary>从源码构建</summary>

**环境要求：** Rust 1.85+、`PATH` 中可找到 `adb`、已开启 USB 调试。

### Linux

```bash
# Debian/Ubuntu
sudo apt install libavcodec-dev libavformat-dev libavutil-dev libswscale-dev \
                 libopus-dev libasound2-dev pkg-config

# Arch Linux
sudo pacman -S ffmpeg opus alsa-lib

# Fedora/RHEL
sudo dnf install ffmpeg-devel opus-devel alsa-lib-devel
```

本项目要求 FFmpeg `8.x`，通过 `pkg-config --modversion libavcodec` 确认版本。若发行版仓库没有 `8.x`，从源码编译 FFmpeg：

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

VAAPI 硬件解码需安装对应 GPU 驱动（Intel/AMD）。

### Windows

使用 MSVC toolchain，通过 `vcpkg` 安装 FFmpeg 和 Opus。仓库已包含 `.github/vcpkg-triplets/x64-windows-release.cmake`。

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

NVDEC 需要安装 NVIDIA CUDA Toolkit 并设置 `CUDA_PATH`。若不需要，将 `ffmpeg[nvcodec]` 替换为 `ffmpeg` 即可。

### 运行

```bash
git clone https://github.com/keivry/saide.git
cd saide
cargo run --release
```

</details>

## 配置概览

配置文件查找顺序：平台配置目录 → `./config.toml` → 若都不存在则在标准路径创建默认配置。参考 [`config.toml`](config.toml) 示例。

| 配置节             | 用途                                                     |
| ------------------ | -------------------------------------------------------- |
| `[general]`        | 键鼠开关、工具栏、窗口尺寸、绑定地址、scrcpy server 路径 |
| `[scrcpy.video]`   | 码率、帧率、最大尺寸、编解码器                           |
| `[scrcpy.audio]`   | 音频开关、编码、来源、缓冲                               |
| `[scrcpy.options]` | 熄屏与常亮选项                                           |
| `[gpu]`            | 渲染后端（`VULKAN`/`OPENGL`）、垂直同步、硬解开关        |
| `[input]`          | 长按、拖拽阈值、拖拽发送间隔                             |
| `[mappings]`       | 开关键与按设备划分的 profile                             |
| `[logging]`        | 日志级别                                                 |

完整说明见：[docs/configuration.md](docs/configuration.md)

## 映射示例

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

## 故障排除

**找不到 `adb`** — 安装 Android platform-tools 并将 `adb` 加入 `PATH`。

**没有音频** — 需要 Android 11 / API 30+。更旧的设备上 SAide 会自动回退到仅视频+控制模式。

**延迟高** — 尝试降低 `scrcpy.video.max_fps` / `max_size`，增大 `scrcpy.audio.buffer_frames` / `ring_capacity`，或关闭 `gpu.vsync`。

**主题** — 设置 `SAIDE_THEME=dark|light|auto` 可覆盖自动检测的主题。

## 贡献者指南

```bash
cargo fmt --all -- --check
cargo clippy -- -D warnings
cargo test --quiet
```

示例：`test_connection`、`test_audio`、`audio_diagnostic`、`render_avsync`、`probe_codec`、`test_protocol`、`test_auto_decoder`、`test_audio_native`、`test_i18n`、`test_vulkan_import`。

请使用约定式提交前缀（`feat:`、`fix:`、`docs:`、`refactor:`），修改代码时同步更新相关文档与示例。

## 许可证

MIT OR Apache-2.0。详见 [LICENSE-MIT](LICENSE-MIT) 与 [LICENSE-APACHE](LICENSE-APACHE)。
