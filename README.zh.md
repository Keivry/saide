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
- 一键截图和录屏，本地保存为 PNG / MP4

## 反检测系统

SAide 内置行为模拟引擎，对所有输入操作进行拟人化处理，降低被游戏反作弊系统检测的风险。

**核心能力：**

- **坐标抖动** — 触摸坐标随机偏移（±0.5%–±5%），多指操作间距自然变化，避免每次都命中最精确像素
- **延迟随机化** — 操作间插入符合高斯分布的真实延迟（20–500 ms），TouchDown→TouchUp 按压内延迟（30–200 ms）
- **贝塞尔路径平滑** — 滑动轨迹使用三次贝塞尔曲线替代直线，模拟人类手指自然弧度
- **逐字文本键入** — 文本分字符逐个发送，字符间间隔随机变化，非一次性粘贴
- **触摸压力变化** — `pressure` 字段以高斯分布随机化（0.3–1.0），替代固定的 1.0
- **Pointer ID 交替** — 跨操作轮换多个 pointer_id，避免单一 ID 成为追踪指纹
- **生理性微抖动** — TouchMove 和长按期间叠加 8–12Hz 微震颤（0.5–2 px），模拟肌肉自然抖动
- **会话节奏管理** — 引入宏观活跃度波动周期（5–15 分钟），间歇性停顿（2–10 秒），模拟注意力自然衰减
- **速率限制** — 令牌桶限速器防止超人类反应速度的操作爆发
- **停滞检测** — 监控视频帧，检测并警告过于规律一致的输入模式

**配置方式：**

在 `config.toml` 中启用反检测：

```toml
[behavior]
preset = "balanced"   # conservative | balanced | aggressive
enabled = true
```

| 预设              | 抖动   | 延迟 (ms)    | 路径平滑 | 逐字键入 | 压力变化 | 微抖动 |
| ----------------- | ------ | ------------ | -------- | -------- | -------- | ------ |
| `conservative` 保守 | ±0.5% | 0            | 关闭     | 关闭     | 关闭     | 关闭   |
| `balanced` 均衡     | ±3%   | 80 (20–200)  | 开启     | 开启     | 开启     | 开启   |
| `aggressive` 激进   | ±5%   | 200 (50–500) | 开启     | 开启     | 开启     | 开启   |

每个选项均可单独调节，完整参考见 [`config.behavior-example.toml`](config.behavior-example.toml)。当 `[behavior]` 节不存在时，SAide 回退至保守默认值，无额外延迟。

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
| `[behavior]`       | 反检测：触摸、输入、时序拟人化                           |
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

示例：`test_connection`、`test_audio`、`audio_diagnostic`、`render_avsync`、`probe_codec`、`test_protocol`、`test_auto_decoder`、`test_audio_native`、`test_i18n`、`test_planar_interleave`、`test_vulkan_import`。

请使用约定式提交前缀（`feat:`、`fix:`、`docs:`、`refactor:`），修改代码时同步更新相关文档与示例。

## 许可证

MIT OR Apache-2.0。详见 [LICENSE-MIT](LICENSE-MIT) 与 [LICENSE-APACHE](LICENSE-APACHE)。
