# SAide

[![Rust](https://img.shields.io/badge/rust-1.85+-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

> **S**crcpy companion **A**pp with key/mouse mapp**i**ng - **A**n**de**

**SAide** 是一款基于 Rust 开发的高性能 Android 设备镜像与控制应用。通过 USB 或 Wi-Fi 连接 Android 设备,提供低延迟视频流、音频捕获和可自定义的键盘/鼠标映射功能。

[English](README.md) | [文档](docs/) | [配置指南](docs/configuration.md)

---

## 功能特性

### 核心能力

- 🚀 **低延迟串流**: Phase 3 优化后实现 20-35ms 端到端延迟
  - 硬件加速视频解码 (VAAPI/D3D11VA、NVDEC、软件 H.264 回退)
  - 跨平台支持: Linux (VAAPI)、Windows (D3D11VA)、全平台 (NVDEC/软件解码)
  - 优化音频管线 (64 帧缓冲、可配置环形缓冲区)
  - TCP_QUICKACK 和网络优化
- 🎮 **高级输入映射**: 可自定义键盘和鼠标映射
  - 每个按键支持触摸坐标映射,支持屏幕旋转
  - 拖拽检测、长按识别、自适应阈值
  - 按 F10 开关映射(可配置)
- 🎵 **音频串流**: Android 11+ 设备实时音频捕获
  - 支持 Opus 音频,可配置延迟 (1.3-5.3ms @ 48kHz)
  - 无锁环形缓冲区,无杂音播放
- 🖥️ **现代化 UI**: 基于 egui 的跨平台桌面界面
  - 实时 FPS 和延迟指示器
  - 可视化映射编辑器
  - 编解码器兼容性检测对话框与进度窗口
- 🌐 **国际化**: 完整支持中英文语言环境
  - 自动语言检测 (系统 `$LANG`)
  - Debug 模式下支持热重载

### 技术亮点

- **零拷贝 GPU 渲染** 通过 wgpu (Vulkan/DirectX 12 后端)
- **硬件加速**: 
  - Linux: VAAPI (Intel/AMD)、NVDEC (NVIDIA)
  - Windows: D3D11VA (Intel/AMD/NVIDIA)、NVDEC (NVIDIA)
  - 全平台: 软件 H.264 回退
- **稳健的错误处理**: 对预期运行时故障提供完整诊断信息
- **一切皆可配置**: 基于 TOML 的配置系统,带验证和热重载
- **内置性能分析**: 5 阶段延迟分析 (网络 → 解码 → 上传 → 显示)

---

## 快速开始

### 前置要求

#### 系统需求

- **操作系统**: Linux (已测试),Windows (实验性 - v0.3),macOS (规划中 - v0.3)
- **Rust**: 1.85 或更高版本
- **Android**: Android 5.0+ 设备 (音频需 Android 11+)
- **ADB**: 已安装 Android Debug Bridge 并在 PATH 中

#### Linux 依赖

安装 FFmpeg 开发库和图形驱动:

```bash
# Debian/Ubuntu
sudo apt install libavcodec-dev libavformat-dev libavutil-dev libswscale-dev \
                 libopus-dev libasound2-dev pkg-config

# Arch Linux
sudo pacman -S ffmpeg opus alsa-lib

# Fedora/RHEL
sudo dnf install ffmpeg-devel opus-devel alsa-lib-devel
```

**硬件加速** (可选但推荐):

```bash
# Intel/AMD (VAAPI)
sudo apt install libva-dev mesa-va-drivers

# NVIDIA (NVDEC) - 需要专有驱动
# 从此处安装: https://www.nvidia.com/Download/index.aspx
```

### 安装

#### 1. 克隆仓库

```bash
git clone https://github.com/yourusername/saide.git
cd saide
```

#### 2. 从源码构建

```bash
# Debug 构建 (快速编译,未优化)
cargo build

# Release 构建 (优化,运行速度快 3 倍)
cargo build --release
```

#### 3. 运行 SAide

```bash
# 使用 cargo (debug)
cargo run

# 或直接运行 (release)
./target/release/saide
```

---

## 使用方法

### 基本工作流

1. **通过 USB 连接 Android 设备** (在开发者选项中启用 USB 调试)
2. **启动 SAide**:
   ```bash
   cargo run --release
   ```
3. **在设备上授权 ADB** (出现提示时)
4. SAide 将自动:
   - 部署 scrcpy-server 到设备
   - 建立视频/音频/控制流
   - 在应用窗口中开始镜像

### 键盘映射

创建或编辑 `~/.config/saide/config.toml`:

```toml
[[mappings.key]]
key = "W"              # 按下的 PC 键
android_key = "UP"     # 发送的 Android 键 (可选)
touch_x = 0.5          # 触摸 x 坐标 (0.0-1.0,归一化)
touch_y = 0.3          # 触摸 y 坐标 (0.0-1.0,归一化)
action = "both"        # "down"、"up" 或 "both"
```

**示例**: 手游 WASD 移动控制:

```toml
[mappings]
toggle = "F10"         # 按 F10 启用/禁用映射
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

### 音频配置

通过调整缓冲区大小降低音频延迟:

```toml
[scrcpy.audio]
enabled = true
buffer_frames = 64       # 更低 = 更少延迟 (64 ≈ 1.3ms @ 48kHz)
                         # 更高 = 更少杂音 (128/256)
ring_capacity = 5760     # 如遇音频丢帧请增大此值
```

**故障排除**:

- **音频爆音/丢帧**: 将 `buffer_frames` 增大到 128 或 256
- **延迟过高**: 将 `buffer_frames` 减小到 32 或 64 (弱硬件可能导致欠载)

详细配置请参阅 [配置指南](docs/configuration.md)。

---

## 配置文件

SAide 使用 TOML 配置文件,位置:

- **Linux**: `~/.config/saide/config.toml`
- **macOS**: `~/Library/Application Support/saide/config.toml`
- **Windows**: `%APPDATA%\saide\config.toml`

### 主要配置节

| 配置节           | 用途                                 | 文档                                                                               |
| ---------------- | ------------------------------------ | ---------------------------------------------------------------------------------- |
| `[general]`      | 窗口大小、超时、绑定地址             | [docs/configuration.md](docs/configuration.md#general---general-settings)          |
| `[scrcpy.video]` | 比特率、FPS、分辨率、编解码器        | [docs/configuration.md](docs/configuration.md#scrcpyvideo---video-stream-settings) |
| `[scrcpy.audio]` | 缓冲区大小、环形缓冲区容量、编解码器 | [docs/configuration.md](docs/configuration.md#scrcpyaudio---audio-stream-settings) |
| `[gpu]`          | 后端 (Vulkan/OpenGL)、垂直同步       | [docs/configuration.md](docs/configuration.md#gpu---gpu-rendering)                 |
| `[input]`        | 长按、拖拽阈值、更新间隔             | [docs/configuration.md](docs/configuration.md#input---input-control-settings)      |
| `[mappings]`     | 键盘/鼠标映射                        | [docs/configuration.md](docs/configuration.md#mappings---keyboard-mapping)         |

**配置示例**: [config.toml](config.toml)

---

## 开发

### 项目结构

```
saide/
├── src/
│   ├── main.rs              # 应用程序入口点
│   ├── lib.rs               # 库导出
│   ├── core/                # UI 层与应用逻辑 (egui)
│   │   ├── ui/              # app.rs、editor.rs、dialog.rs、player.rs 等
│   │   ├── coords/          # 坐标映射 (屏幕 ↔ 设备)
│   │   ├── init.rs          # 初始化协调器
│   │   ├── connection.rs    # 连接管理
│   │   ├── device_monitor.rs # ADB 设备监控
│   │   └── state.rs         # 应用状态机
│   ├── controller/          # 输入处理
│   │   ├── keyboard.rs      # 键盘映射器
│   │   ├── mouse.rs         # 鼠标映射器
│   │   └── adb.rs           # ADB shell 封装
│   ├── scrcpy/              # Scrcpy 协议
│   │   ├── protocol/        # 视频/音频/控制数据包
│   │   ├── connection.rs    # TCP 流管理器
│   │   └── server.rs        # scrcpy-server 部署
│   ├── decoder/             # 视频/音频解码
│   │   ├── h264.rs          # 软件 H.264 解码器
│   │   ├── nvdec.rs         # NVIDIA 硬件解码器
│   │   ├── vaapi.rs         # VAAPI 硬件解码器
│   │   └── audio/           # Opus 音频解码与播放
│   ├── config/              # 配置管理
│   ├── i18n/                # 国际化
│   └── profiler/            # 延迟分析
├── docs/                    # 文档
│   ├── ARCHITECTURE.md      # 系统架构
│   ├── configuration.md     # 配置指南
│   ├── LATENCY_OPTIMIZATION.md  # 性能调优
│   └── pitfalls.md          # 已知问题与解决方案
├── examples/                # 示例程序
└── config.toml              # 默认配置
```

### 运行测试

```bash
# 运行所有测试
cargo test

# 详细输出
cargo test -- --nocapture

# 运行特定测试
cargo test test_audio_decode
```

### 代码质量检查

```bash
# 格式化代码
cargo fmt --all

# Clippy 检查 (严格模式)
cargo clippy -- -D warnings

# 检查格式 + Lint
cargo fmt --all -- --check && cargo clippy -- -D warnings
```

### 示例程序

SAide 包含多个独立示例用于测试组件:

```bash
# 测试 scrcpy 连接 (无 UI)
cargo run --example test_connection

# 测试音频解码和播放
cargo run --example test_audio

# 音频诊断 (延迟测量)
cargo run --example audio_diagnostic

# 音视频同步测试 (统计信息)
cargo run --example render_avsync
```

示例程序会按顺序在应用数据目录、当前工作目录以及旧版仓库常见的 `3rd-party/` 目录中查找 `scrcpy-server-v3.3.3`。

完整列表请参阅 [examples/](examples/)。

---

## 性能

### 延迟分解 (Phase 3 优化)

| 阶段         | 优化前  | 优化后      | 优化手段                          |
| ------------ | ------- | ----------- | --------------------------------- |
| **网络**     | 15-25ms | 10-15ms     | TCP_QUICKACK、移除 flush          |
| **解码**     | 10-15ms | 8-12ms      | FFmpeg 标志 (FAST + EXPERIMENTAL) |
| **音频缓冲** | 2.7ms   | 1.3ms       | 128→64 帧 @ 48kHz                 |
| **GPU 上传** | 8-12ms  | 8-12ms      | (Phase 2 延后 - wgpu 限制)        |
| **显示**     | 5-10ms  | 5-10ms      | 默认禁用垂直同步                  |
| **总计**     | 50-70ms | **20-35ms** | ✅ 目标达成                       |

**性能分析**: 内置延迟分析器追踪所有 5 个阶段,提供 P50/P95 统计。启用方法:

```toml
[logging]
level = "debug"  # 显示每帧延迟统计
```

详细分析请参阅 [docs/LATENCY_OPTIMIZATION.md](docs/LATENCY_OPTIMIZATION.md)。

---

## 路线图

### 已完成 (v0.1)

- ✅ 基础视频/音频串流
- ✅ 硬件加速解码 (VAAPI、NVDEC)
- ✅ 键盘/鼠标映射,支持旋转
- ✅ 配置系统,带验证
- ✅ 国际化 (zh_CN、en_US)
- ✅ 延迟优化 (Phase 1 + 3.1)

### 计划中

#### 近期 (v0.2 - 2026 Q1)

- [ ] 设置面板 (GPU 后端、编解码器、音频调优)
- [ ] 日志查看器 (集成 tracing-appender)
- [ ] 映射配置导入/导出
- [ ] 剪贴板同步 (Android ↔ PC)

#### 中期 (v0.3 - 2026 Q2)

- [ ] H.265/AV1 编解码器支持
- [ ] 文件传输 (拖放文件到设备)
- [ ] 录制模式 (保存视频/音频到文件)
- [ ] macOS/Windows 支持

#### 长期 (v1.0 - 2026+)

- [ ] Wi-Fi 无线连接 (无需 USB)
- [ ] 多设备支持 (同时镜像多台设备)
- [ ] 插件系统 (自定义输入映射)
- [ ] 脚本 API (自动化设备控制)

详细任务请参阅 [TODO.md](TODO.md)。

---

## 故障排除

### 常见问题

#### 1. **"ADB not found in PATH"**

**解决方案**: 安装 Android SDK Platform-Tools:

```bash
# Debian/Ubuntu
sudo apt install android-tools-adb

# macOS
brew install android-platform-tools

# 或从此处下载: https://developer.android.com/tools/releases/platform-tools
```

#### 2. **"Device unauthorized"**

**解决方案**: 检查 Android 设备屏幕上的 "允许 USB 调试" 提示,授权该计算机。

#### 3. **黑屏 / 无视频**

**可能原因**:

- 设备屏幕已关闭 (检查配置中的 `turn_screen_off`)
- 编解码器不匹配 (设备不支持 H.264)
- 未安装 FFmpeg

**解决方案**:

```toml
[scrcpy.options]
turn_screen_off = false  # 保持设备屏幕开启
```

检查设备支持的编解码器:

```bash
cargo run --example probe_codec
```

#### 4. **音频爆音 / 丢帧**

**解决方案**: 增大音频缓冲区:

```toml
[scrcpy.audio]
buffer_frames = 128      # 或 256 (弱硬件)
ring_capacity = 11520    # 双倍默认值
```

#### 5. **CPU 占用过高**

**可能原因**:

- 软件解码 (无 GPU 加速)
- 高 FPS/分辨率

**解决方案**:

```toml
[scrcpy.video]
max_fps = 30            # 从 60 降低
max_size = 1280         # 从 1920 降低

[gpu]
backend = "VULKAN"      # 确保硬件加速
```

检查 GPU 检测:

```bash
cargo run 2>&1 | grep "Video backend"
# 应显示: "Video backend: VULKAN"
```

#### 6. **输入延迟 / 控制卡顿**

**解决方案**: 降低输入阈值:

```toml
[input]
long_press_ms = 200      # 更快长按检测
drag_threshold_px = 3.0  # 更敏感的拖拽
drag_interval_ms = 4     # 更高更新率 (240fps)
```

### 调试模式

启用详细日志:

```bash
RUST_LOG=debug cargo run
```

或在 `config.toml` 中:

```toml
[logging]
level = "debug"
```

### 已知问题

#### Windows 特定问题 (v0.3 - 实验性)

- **GPU 检测返回 "Unknown"**: D3D11VA 仍可工作,但解码器选择未优化,DXGI 枚举待实现。
- **首次运行可能较慢**: Windows Defender 可能在首次启动时扫描可执行文件。
- **配置文件路径**: 使用 `%APPDATA%\saide\config.toml` 而非 `~/.config/saide/config.toml`。
- **分辨率变化时连接断开 (2026-01-27)**: ✅ **已在 v0.3.1 修复**
  - **症状**: 设备旋转后约 2.5 秒视频流断开
  - **根因**: TOCTTOU 竞态条件 - `try_send()` 失败后检查 `is_full()`,但 UI 线程在两次调用间消费了帧,导致误判为已断开
  - **修复**: 直接匹配 `TrySendError::{Full, Disconnected}` 而非事后检查 `is_full()`
  - **影响**: 消除 Windows 和 Linux 上的误断开。Windows 因整体性能较慢(缓冲区满状态持续更久)触发更频繁,但本质是跨平台 Bug。
- **AMD GPU D3D11VA 硬编码配置索引 (2026-01-27)**: ✅ **已在 d7f0b25 修复**
  - **之前问题**: 硬编码 `avcodec_get_hw_config(..., 0)` 导致部分 FFmpeg 构建初始化失败
  - **症状**: 报错 `Failed setup for format d3d11: hwaccel initialisation returned error` 或 `Hardware config mismatch`
  - **修复**: 现在遍历所有 hw_config 索引直到找到 D3D11VA
  - **增强诊断**: 添加 FFmpeg 错误信息转换 (`av_strerror`) 并提供可操作的建议
  - **当前状态**: 大多数 AMD GPU 在正确驱动下已可正常使用 D3D11VA
  - **如仍失败**: 运行 `.\scripts\test_d3d11va_amd.ps1` 诊断驱动/UMA 显存问题 (详见 [AMD_D3D11VA_TROUBLESHOOTING.md](docs/AMD_D3D11VA_TROUBLESHOOTING.md))

完整的已知问题和解决方法请参阅 [docs/pitfalls.md](docs/pitfalls.md)。

---

## 贡献

欢迎贡献! 请遵循以下指南:

1. **代码风格**: 提交前运行 `cargo fmt`
2. **Lint 检查**: 确保 `cargo clippy -- -D warnings` 通过
3. **测试**: 为新功能添加测试
4. **文档**: 更新相关文档 (README、config.md、architecture.md)
5. **提交信息**: 使用约定式提交 (如 `feat: add H.265 support`)

### 开发工作流

```bash
# 1. 创建功能分支
git checkout -b feature/my-feature

# 2. 修改代码
# ... 编辑代码 ...

# 3. 检查代码质量
cargo fmt --all
cargo clippy -- -D warnings
cargo test

# 4. 提交
git add .
git commit -m "feat: 描述你的修改"

# 5. 推送并创建 PR
git push origin feature/my-feature
```

---

## 许可证

本项目采用 MIT 许可证 - 详见 [LICENSE](LICENSE) 文件。

---

## 致谢

- **[scrcpy](https://github.com/Genymobile/scrcpy)**: 灵感来源和协议参考
- **[FFmpeg](https://ffmpeg.org/)**: 视频/音频解码
- **[egui](https://github.com/emilk/egui)**: 即时模式 GUI 框架
- **[wgpu](https://github.com/gfx-rs/wgpu)**: 跨平台 GPU API

---

## 联系方式

- **问题反馈**: [GitHub Issues](https://github.com/yourusername/saide/issues)
- **讨论**: [GitHub Discussions](https://github.com/yourusername/saide/discussions)

---

**用 ❤️ 和 Rust 制作**
