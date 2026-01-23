# SAide 跨平台支持路线图

> **创建日期**: 2026-01-21  
> **目标**: 支持 Linux 跨桌面环境 (GNOME/KDE/Xfce) 和跨系统 (Windows/macOS) 运行  
> **当前状态**: 仅在 Gentoo Linux + KDE Plasma 6 Wayland 环境下测试

---

## 目录

1. [执行摘要](#执行摘要)
2. [现状分析](#现状分析)
3. [架构设计](#架构设计)
4. [实施计划](#实施计划)
5. [模块详细设计](#模块详细设计)
6. [依赖管理](#依赖管理)
7. [测试策略](#测试策略)
8. [风险与缓解](#风险与缓解)
9. [时间线](#时间线)

---

## 执行摘要

### 当前限制

SAide 目前仅在以下环境测试和运行：
- **操作系统**: Gentoo Linux
- **桌面环境**: KDE Plasma 6
- **显示协议**: Wayland
- **硬件解码**: VAAPI (Intel/AMD), NVDEC (NVIDIA)

### 目标平台

| 优先级 | 平台 | 桌面环境 | 状态 | 预计工时 |
|--------|------|----------|------|----------|
| **P0** | Windows 10/11 | N/A | 未支持 | 3-4 周 |
| **P0** | macOS 12+ | N/A | 未支持 | 2-3 周 |
| **P1** | Linux | GNOME (Wayland/X11) | 需测试 | 1 周 |
| **P1** | Linux | Xfce (X11) | 需测试 | 1 周 |
| **P1** | Linux | Sway (Wayland) | 需测试 | 1 周 |

### 核心改动

```
主要修改模块:
├─ src/decoder/
│   ├─ windows_mf.rs      (新增) - Windows Media Foundation 解码器
│   ├─ macos_vt.rs        (新增) - macOS VideoToolbox 解码器
│   └─ auto.rs            (修改) - 平台检测 + 自动选择
├─ src/gpu/
│   └─ mod.rs             (修改) - 跨平台 GPU 检测
├─ src/controller/
│   └─ adb.rs             (修改) - 跨平台进程管理
├─ Cargo.toml             (修改) - 添加条件依赖
└─ src/config/
    └─ mod.rs             (修改) - GPU 后端配置扩展
```

---

## 现状分析

### 已发现的平台特定代码

#### 1. 硬件视频解码器 (P0 - 阻塞跨平台)

| 文件 | 问题 | Linux 特定 |
|------|------|------------|
| `src/decoder/vaapi.rs:50` | 硬编码 `/dev/dri/renderD128` | ✅ DRM/KMS |
| `src/decoder/vaapi.rs:67` | 日志引用 Linux 路径 | ✅ |
| `src/decoder/nvdec.rs:54-57` | CUDA 设备创建 | ⚠️ 跨平台但需驱动 |
| `src/gpu/mod.rs:35-78` | `/proc/driver/nvidia`, `/dev/dri`, `/sys/class/drm` | ✅ |

**影响**: Windows/macOS 无硬件加速，必须软件解码

#### 2. 网络优化 (P3 - 性能优化)

| 文件 | 问题 | 平台特定 |
|------|------|----------|
| `src/scrcpy/connection.rs:473-499` | `TCP_QUICKACK` | Linux only |

**影响**: Windows/macOS 损失 10-15ms 延迟优化

#### 3. 进程管理 (P2 - 功能完整)

| 文件 | 问题 | 平台特定 |
|------|------|----------|
| `src/controller/adb.rs` | 进程控制依赖 | Windows 仍需补齐实现 |

**影响**: Windows ADB 进程控制可能失效

#### 4. GPU 渲染 (P1 - 已设计)

| 依赖 | 当前状态 | 跨平台支持 |
|------|----------|------------|
| `wgpu` | Vulkan only | 需启用 dx12/metal |

**影响**: Windows/macOS 可用 wgpu，但需配置后端

#### 5. 音频播放 (✅ 已跨平台)

| 依赖 | 状态 |
|------|------|
| `cpal` | ✅ 跨平台 (ALSA/WASAPI/CoreAudio) |
| `opus` | ✅ 跨平台 |

---

## 架构设计

### 跨平台解码器抽象

```
┌─────────────────────────────────────────────────────────────┐
│                    VideoDecoder Trait                        │
├─────────────────────────────────────────────────────────────┤
│ fn new(width, height) -> Result<Self>                        │
│ fn decode_packet(&mut self, packet) -> Result<DecodedFrame>  │
│ fn flush(&mut self)                                          │
└─────────────────────────────────────────────────────────────┘
                              ▲
                              │
          ┌───────────────────┼───────────────────┐
          │                   │                   │
          ▼                   ▼                   ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
│ VaapiDecoder    │ │ NvdecDecoder    │ │ H264Decoder     │
│ (Linux DRM)     │ │ (NVIDIA CUDA)   │ │ (Software)      │
└─────────────────┘ └─────────────────┘ └─────────────────┘
          │                   │                   │
          ▼                   ▼                   ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
│ WindowsMfDecoder│ │ MacosVtDecoder  │ │                 │
│ (新增)           │ │ (新增)           │ │                 │
└─────────────────┘ └─────────────────┘ └─────────────────┘
```

### 自动解码器选择策略

```rust
// src/decoder/auto.rs

pub fn create_decoder(config: &VideoConfig) -> Result<Box<dyn VideoDecoder>> {
    // 1. 检测当前平台
    let platform = detect_platform();

    // 2. 按优先级尝试硬件解码器
    match platform {
        Platform::Windows => {
            // 优先级: D3D11VA → Media Foundation → Software
            if let Ok(decoder) = WindowsMfDecoder::new(config.width, config.height) {
                return Ok(Box::new(decoder));
            }
            // 回退到软件解码
            H264Decoder::new(config.width, config.height)
        }
        Platform::MacOS => {
            // 优先级: VideoToolbox → Software
            if let Ok(decoder) = MacosVtDecoder::new(config.width, config.height) {
                return Ok(Box::new(decoder));
            }
            H264Decoder::new(config.width, config.height)
        }
        Platform::Linux => {
            // 优先级: NVDEC → VAAPI → Software
            if config.hardware_acceleration_disabled {
                return H264Decoder::new(config.width, config.height);
            }

            // 尝试 NVDEC (NVIDIA)
            if let Ok(decoder) = NvdecDecoder::new(config.width, config.height) {
                return Ok(Box::new(decoder));
            }

            // 尝试 VAAPI (Intel/AMD)
            if let Ok(decoder) = VaapiDecoder::new(config.width, config.height) {
                return Ok(Box::new(decoder));
            }

            // 回退到软件解码
            H264Decoder::new(config.width, config.height)
        }
    }
}
```

---

## 实施计划

### 阶段 1: Windows 支持 (3-4 周)

#### Week 1: Media Foundation 解码器

```
任务: 创建 src/decoder/windows_mf.rs

文件结构:
src/decoder/
└── windows/
    └── mf.rs           (新增 ~400 行)

依赖:
├─ ffmpeg-next 7.1     (已有,启用 Media Foundation)
├─ windows-sys 0.52    (新增,Windows API 绑定)
└─ winapi 0.3          (可选,与 windows-sys 二选一)

关键 API:
├─ MFCreateMediaSession
├─ MFCreateSourceReaderFromURL
├─ IMFMediaSource
└─ IMFSourceReader
```

**实现步骤**:

1. **初始化 Media Foundation**
```rust
// windows_mf.rs

use windows_sys::{
    MediaFoundation::{MFCreateMediaSession, IMFMediaSession},
    Windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED},
};

pub struct WindowsMfDecoder {
    session: IMFMediaSession,
    reader: IMFSourceReader,
    // ...
}

impl WindowsMfDecoder {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        // 初始化 COM
        unsafe {
            CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED)?;
        }

        // 创建 Media Session
        let session = unsafe {
            let mut session: *mut std::ffi::c_void = std::ptr::null_mut();
            MFCreateMediaSession(std::ptr::null(), &mut session)?;
            IMFMediaSession::from_raw(session)
        };

        Ok(Self { session, reader: /* ... */ })
    }
}
```

2. **配置硬件加速**
```rust
impl WindowsMfDecoder {
    fn configure_hardware_acceleration(&mut self) -> Result<()> {
        // 启用 D3D11 硬件加速
        unsafe {
            let attributes = create_dxgi_device_manager()?;
            self.reader.SetUIThreadDispatchQueue(attributes)?;
        }
        Ok(())
    }
}
```

3. **实现帧解码**
```rust
impl VideoDecoder for WindowsMfDecoder {
    fn decode_packet(&mut self, packet: &[u8]) -> Result<DecodedFrame> {
        // 将 H.264 帧送入 Media Foundation
        // 读取解码后的帧
        // 转换为 NV12/RGBA 格式
    }
}
```

#### Week 2: GPU 后端配置 + 进程管理

```
任务:
├─ 修改 Cargo.toml 添加 windows-sys 依赖
├─ 修改 src/gpu/mod.rs 添加 DXGI GPU 检测
└─ 修改 src/controller/adb.rs 添加 Windows 进程控制
```

**Cargo.toml 修改**:
```toml
[target.'cfg(windows)'.dependencies]
windows-sys = "0.52"
winapi = { version = "0.3", features = [
    "dxgi",
    "d3d11",
    "mfapi",
    "mfobjects",
] }

```

**Windows GPU 检测**:
```rust
// src/gpu/windows.rs (新增)

use windows_sys::Win32::Graphics::Dxgi::{
    IDXGIAdapter, DXGI_ADAPTER_DESC,
    CreateDXGIFactory, DXGI_FORMAT_NV12,
};

pub fn detect_gpu_windows() -> GpuType {
    unsafe {
        let factory = CreateDXGIFactory::<IDXGIFactory>()?;
        let mut adapter: *mut IDXGIAdapter = std::ptr::null_mut();

        // 枚举所有 GPU
        for i in 0.. {
            if factory.EnumAdapters(i, &mut adapter) != 0 {
                break;
            }

            let mut desc: DXGI_ADAPTER_DESC = std::mem::zeroed();
            adapter.GetDesc(&mut desc);

            match desc.VendorId {
                0x10DE => return GpuType::Nvidia,  // NVIDIA
                0x8086 => return GpuType::Intel,   // Intel
                0x1002 => return GpuType::Amd,     // AMD
                _ => continue,
            }
        }
    }
    GpuType::Unknown
}
```

#### Week 3-4: 测试 + Bug 修复

```
测试项目:
├─ Windows 10 (20H2+)
├─ Windows 11 (22H2+)
├─ NVIDIA GPU (RTX 3060+)
├─ Intel GPU (Xe/Arc)
├─ AMD GPU (RX 6000/7000)
└─ 软件解码回退
```

### 阶段 2: macOS 支持 (2-3 周)

#### Week 1: VideoToolbox 解码器

```
任务: 创建 src/decoder/macos_vt.rs

依赖:
├─ ffmpeg-next 7.1  (已有,启用 VideoToolbox)
└─ core-foundation 0.10  (新增)
```

**关键 API**:
```rust
// macos_vt.rs

use core_foundation::{CFArray, CFString};

pub struct MacosVtDecoder {
    session: VTDecompressionSessionRef,
    format_desc: CMVideoFormatDescriptionRef,
    // ...
}

impl MacosVtDecoder {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        // 创建 Video Toolbox 会话
        // 配置硬件解码器
        // 返回解码器实例
    }
}
```

#### Week 2-3: 测试 + 优化

```
测试项目:
├─ macOS 12 Monterey+
├─ macOS 13 Ventura+
├─ macOS 14 Sonoma+
├─ Apple Silicon (M1/M2/M3)
├─ Intel Mac (Core i5/i7)
└─ 软件解码回退
```

### 阶段 3: 跨桌面环境支持 (1-2 周)

#### Linux 桌面环境兼容性

```
当前已知问题:
├─ /dev/dri/renderD128 路径可能不同
│  ├─ Ubuntu/Debian: /dev/dri/renderD128 (标准)
│  ├─ Arch Linux: /dev/dri/renderD128 (标准)
│  ├─ Fedora: /dev/dri/renderD128 (标准)
│  └─ 非标准路径: 需要动态枚举
│
├─ Wayland vs X11 差异
│  ├─ egui + wgpu 已抽象窗口系统
│  ├─ 输入事件由 egui 处理
│  └─ 无需额外适配
│
└─ 权限问题
   ├─ /dev/dri 访问需要 video 组权限
   └─ 需检查用户组并提示
```

**动态设备枚举**:
```rust
// src/decoder/vaapi.rs

fn find_vaapi_device() -> Option<PathBuf> {
    // 1. 标准路径
    let standard = PathBuf::from("/dev/dri/renderD128");
    if standard.exists() {
        return Some(standard);
    }

    // 2. 动态枚举 /dev/dri
    if let Ok(entries) = fs::read_dir("/dev/dri") {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("renderD") {
                return Some(entry.path());
            }
        }
    }

    // 3. 尝试 VA-API VADisplay 间接发现
    // 通过 libva 查询设备

    None
}
```

#### 桌面环境测试矩阵

| 桌面环境 | 显示协议 | 测试状态 | 已知问题 |
|----------|----------|----------|----------|
| KDE Plasma 6 | Wayland | ✅ 已测试 | 无 |
| GNOME 45+ | Wayland | ⚠️ 待测试 | 预计兼容 |
| GNOME 45+ | X11 | ⚠️ 待测试 | 预计兼容 |
| Xfce 4.18 | X11 | ⚠️ 待测试 | 预计兼容 |
| Sway 1.9 | Wayland | ⚠️ 待测试 | 预计兼容 |

---

## 模块详细设计

### 1. Windows Media Foundation 解码器

#### 文件: `src/decoder/windows/mf.rs`

```rust
//! Windows Media Foundation H.264 解码器
//!
//! 支持:
//! - Windows 10 20H2+ / Windows 11
//! - 硬件加速 (D3D11VA / Media Foundation)
//! - 软件解码回退

use super::{DecodedFrame, VideoDecoder, error::{Result, VideoError}};
use windows_sys::{
    MediaFoundation::{
        MFCreateMediaSession, MFCreateSourceReaderFromMediaSource,
        IMFMediaSession, IMFSourceReader, MF_VIDEO_FORMAT,
    },
    Windows::Win32::Graphics::Direct3D11::{
        ID3D11Device, ID3D11DeviceContext, D3D11_CREATE_DEVICE_FLAG,
        D3D11_SDK_VERSION,
    },
};
use std::ptr::{null, null_mut};

/// Media Foundation 硬件解码器
pub struct WindowsMfDecoder {
    /// Media Foundation 会话
    session: IMFMediaSession,
    
    /// 源读取器
    reader: IMFSourceReader,
    
    /// D3D11 设备 (硬件加速时)
    d3d_device: Option<*mut ID3D11Device>,
    
    /// D3D11 设备上下文
    d3d_context: Option<*mut ID3D11DeviceContext>,
    
    /// 视频宽度
    width: u32,
    
    /// 视频高度
    height: u32,
}

impl WindowsMfDecoder {
    /// 创建新的 Media Foundation 解码器
    pub fn new(width: u32, height: u32) -> Result<Self> {
        // 1. 初始化 COM
        unsafe {
            windows_sys::Windows::Win32::System::Com::CoInitializeEx(
                null(),
                windows_sys::Windows::Win32::System::Com::COINIT_MULTITHREADED,
            )?;
        }

        // 2. 创建 D3D11 设备 (硬件加速)
        let (d3d_device, d3d_context) = Self::create_d3d11_device()?;

        // 3. 创建 Media Foundation 会话
        let session = Self::create_media_session()?;

        // 4. 配置源读取器
        let reader = Self::configure_source_reader(&session, d3d_device)?;

        Ok(Self {
            session,
            reader,
            d3d_device,
            d3d_context,
            width,
            height,
        })
    }

    fn create_d3d11_device() -> Result<(Option<*mut ID3D11Device>, Option<*mut ID3D11DeviceContext>)> {
        // 尝试创建 D3D11 设备
        unsafe {
            let mut device: *mut ID3D11Device = null_mut();
            let mut context: *mut ID3D11DeviceContext = null_mut();

            let result = windows_sys::Windows::Win32::Graphics::Direct3D11::D3D11CreateDevice(
                null(), // 默认适配器
                windows_sys::Windows::Win32::Graphics::Direct3D11::D3D_DRIVER_TYPE_HARDWARE,
                null(),
                D3D11_CREATE_DEVICE_FLAG::D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                null(), // feature levels
                0,
                D3D11_SDK_VERSION,
                &mut device,
                null_mut(),
                &mut context,
            );

            if result >= 0 {
                Ok((Some(device), Some(context)))
            } else {
                // 回退到 WARP 或软件
                Self::create_software_device()
            }
        }
    }

    fn create_media_session() -> Result<IMFMediaSession> {
        unsafe {
            let mut session: *mut std::ffi::c_void = null_mut();
            MFCreateMediaSession(null(), &mut session)?;
            Ok(IMFMediaSession::from_raw(session))
        }
    }

    fn configure_source_reader(
        session: &IMFMediaSession,
        _d3d_device: Option<*mut ID3D11Device>,
    ) -> Result<IMFSourceReader> {
        // 配置 H.264 解码器
        // 设置输出格式 NV12
        // 启用硬件加速
        todo!()
    }
}

impl VideoDecoder for WindowsMfDecoder {
    fn decode_packet(&mut self, packet: &[u8]) -> Result<DecodedFrame> {
        // 发送压缩帧到 Media Foundation
        // 获取解码帧
        // 转换为统一格式
        todo!()
    }

    fn flush(&mut self) {
        // 刷新解码器缓冲区
        todo!()
    }
}

impl Drop for WindowsMfDecoder {
    fn drop(&mut self) {
        // 清理 Media Foundation 资源
        todo!()
    }
}
```

### 2. macOS VideoToolbox 解码器

#### 文件: `src/decoder/macos/vt.rs`

```rust
//! macOS VideoToolbox H.264 解码器
//!
//! 支持:
//! - macOS 12.0+
//! - 硬件加速 (VideoToolbox)
//! - Apple Silicon + Intel Mac
//! - 软件解码回退

use super::{DecodedFrame, VideoDecoder, error::{Result, VideoError}};
use core_foundation::{CFArray, CFString};
use std::ptr::null;

/// VideoToolbox 解码器
pub struct MacosVtDecoder {
    /// 解码会话
    session: VTDecompressionSessionRef,
    
    /// 格式描述
    format_desc: CMVideoFormatDescriptionRef,
    
    /// 视频宽度
    width: u32,
    
    /// 视频高度
    height: u32,
}

impl MacosVtDecoder {
    pub fn new(width: u32, height: u32) -> Result<Self> {
        // 1. 创建视频格式描述
        let format_desc = unsafe {
            let mut desc: CMVideoFormatDescriptionRef = null();
            CMVideoFormatDescriptionCreate(
                kCFAllocatorDefault,
                kCMVideoCodecType_H264,
                width as i32,
                height as i32,
                null(),
                &mut desc,
            )?;
            desc
        };

        // 2. 创建解码会话
        let session = Self::create_decompression_session(format_desc)?;

        Ok(Self {
            session,
            format_desc,
            width,
            height,
        })
    }

    fn create_decompression_session(
        format_desc: CMVideoFormatDescriptionRef,
    ) -> Result<VTDecompressionSessionRef> {
        unsafe {
            let mut session: VTDecompressionSessionRef = null();
            let output_attrs: CFDictionaryRef = create_output_attributes(self.width, self.height)?;

            VTDecompressionSessionCreate(
                kCFAllocatorDefault,
                format_desc,
                null(), // video decoder specification
                output_attrs,
                &mut output_callbacks,
                &mut session,
            )?;

            Ok(session)
        }
    }
}

impl VideoDecoder for MacosVtDecoder {
    fn decode_packet(&mut self, packet: &[u8]) -> Result<DecodedFrame> {
        // 发送 H.264 NALU 到 VideoToolbox
        // 获取 CVPixelBuffer
        // 转换为 NV12/RGBA
        todo!()
    }

    fn flush(&mut self) {
        // 刷新解码器
        todo!()
    }
}
```

### 3. 跨平台 GPU 检测

#### 文件: `src/gpu/mod.rs` (修改)

```rust
//! GPU 检测和类型识别

use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuType {
    Nvidia,
    Intel,
    Amd,
    Apple,      // 新增: Apple Silicon
    Software,   // 新增: 软件渲染
    Unknown,
}

#[cfg(target_os = "linux")]
mod gpu_detector {
    use super::GpuType;
    use std::{fs, path::Path};
    use tracing::debug;

    pub fn detect() -> GpuType {
        // NVIDIA 检测
        if is_nvidia_gpu_available() {
            return GpuType::Nvidia;
        }

        // Intel/AMD 检测
        if let Some(gpu) = detect_drm_gpu() {
            return gpu;
        }

        GpuType::Unknown
    }

    fn is_nvidia_gpu_available() -> bool {
        // 检查 /proc/driver/nvidia/version
        if Path::new("/proc/driver/nvidia/version").exists() {
            return true;
        }

        // 检查 nvidia-smi
        if let Ok(output) = std::process::Command::new("nvidia-smi")
            .arg("--query-gpu=name")
            .arg("--format=csv,noheader")
            .output()
        {
            return output.status.success() && !output.stdout.is_empty();
        }

        false
    }

    fn detect_drm_gpu() -> Option<GpuType> {
        // 动态枚举 /dev/dri
        if let Ok(entries) = fs::read_dir("/dev/dri") {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if name.to_string_lossy().starts_with("renderD") {
                    if let Some(vendor) = get_device_vendor(&entry.path()) {
                        match vendor {
                            0x8086 => return Some(GpuType::Intel),
                            0x1002 => return Some(GpuType::Amd),
                            _ => {}
                        }
                    }
                }
            }
        }
        None
    }

    fn get_device_vendor(path: &Path) -> Option<u32> {
        let vendor_path = path.join("device/vendor");
        fs::read_to_string(&vendor_path).ok()
            .and_then(|s| u32::from_str_radix(s.trim().trim_start_matches("0x"), 16).ok())
    }
}

#[cfg(target_os = "windows")]
mod gpu_detector {
    use super::GpuType;
    use tracing::debug;

    pub fn detect() -> GpuType {
        use windows_sys::Win32::Graphics::Dxgi::*;

        unsafe {
            let factory = CreateDXGIFactory::<IDXGIFactory>().unwrap_or(std::ptr::null_mut());
            if factory.is_null() {
                return GpuType::Software;
            }

            let mut adapter: *mut IDXGIAdapter = std::ptr::null_mut();
            for i in 0.. {
                if factory.EnumAdapters(i, &mut adapter) != 0 {
                    break;
                }

                let mut desc: DXGI_ADAPTER_DESC = std::mem::zeroed();
                adapter.GetDesc(&mut desc);

                match desc.VendorId {
                    0x10DE => return GpuType::Nvidia,
                    0x8086 => return GpuType::Intel,
                    0x1002 => return GpuType::Amd,
                    _ => {}
                }
            }
        }
        GpuType::Unknown
    }
}

#[cfg(target_os = "macos")]
mod gpu_detector {
    use super::GpuType;
    use std::process::Command;

    pub fn detect() -> GpuType {
        // 检测 Apple Silicon
        if let Ok(output) = Command::new("sysctl")
            .arg("machdep.cpu.brand_string")
            .output()
        {
            let brand = String::from_utf8_lossy(&output.stdout);
            if brand.contains("Apple") || brand.contains("M1") || brand.contains("M2") || brand.contains("M3") {
                return GpuType::Apple;
            }
        }

        // Intel Mac
        GpuType::Intel // 或 AMD 如果检测到
    }
}

pub fn detect_gpu() -> GpuType {
    gpu_detector::detect()
}
```

### 4. 条件依赖管理

#### Cargo.toml 修改

```toml
[package]
name = "saide"
version = "0.1.0"
edition = "2024"

[lib]
name = "saide"
path = "src/lib.rs"

[dependencies]

# 核心依赖 (跨平台)
eframe = { version = "0.33", features = ["wgpu"] }
egui = { version = "0.33", features = ["serde"] }
wgpu = { version = "27", default-features = false }

# ... 其他依赖

# 视频解码
ffmpeg-next = "7.1"

# 音频播放
cpal = "0.17"
opus = "0.3"

[target.'cfg(windows)'.dependencies]
windows-sys = { version = "0.52", features = [
    "Win32_Foundation",
    "Win32_System_Threading",
    "Win32_System_ProcessStatus",
] }
winapi = { version = "0.3", features = [
    "dxgi",
    "d3d11",
    "mfapi",
    "mfobjects",
    "mmdeviceapi",
    "audioclient",
] }

[target.'cfg(target_os = "macos")'.dependencies]
core-foundation = "0.10"
core-graphics = "0.24"
objc2 = "0.5"

# wgpu 后端配置
[target.'cfg(target_os = "linux")'.dependencies.wgpu]
version = "27"
features = ["vulkan"]

[target.'cfg(target_os = "windows")'.dependencies.wgpu]
version = "27"
features = ["dx12"]

[target.'cfg(target_os = "macos")'.dependencies.wgpu]
version = "27"
features = ["metal"]

[profile.release]
opt-level = 3
codegen-units = 1
lto = true
strip = true
panic = "abort"
```

---

## 依赖管理

### 新增依赖版本要求

| 依赖 | 版本 | 用途 | 平台 |
|------|------|------|------|
| windows-sys | 0.52 | Windows API 绑定 | Windows |
| winapi | 0.3 | DirectX/MF | Windows |
| core-foundation | 0.10 | macOS Core Foundation | macOS |
| core-graphics | 0.24 | macOS Graphics | macOS |
| objc2 | 0.5 | Objective-C 运行时 | macOS |

### FFmpeg 配置

```toml
# Windows/macOS 需要启用对应硬件加速
[target.'cfg(not(target_os = "linux"))'.dependencies]
ffmpeg-next = { version = "7.1", features = ["nvenc", " videotoolbox"] }

[target.'cfg(target_os = "windows")'.dependencies.ffmpeg-next]
version = "7.1"
features = ["nvenc", "d3d11va"]  # Windows 硬件加速

[target.'cfg(target_os = "macos")'.dependencies.ffmpeg-next]
version = "7.1"
features = ["videotoolbox"]  # macOS 硬件加速
```

---

## 测试策略

### 单元测试

```rust
// tests/platform_detection.rs

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windows_gpu_detection() {
        #[cfg(target_os = "windows")]
        {
            let gpu = detect_gpu();
            assert!(matches!(gpu, GpuType::Nvidia | GpuType::Intel | GpuType::Amd | GpuType::Unknown));
        }
    }

    #[test]
    fn test_macos_gpu_detection() {
        #[cfg(target_os = "macos")]
        {
            let gpu = detect_gpu();
            // Apple Silicon 或 Intel
            assert!(matches!(gpu, GpuType::Apple | GpuType::Intel | GpuType::Unknown));
        }
    }
}
```

### 集成测试

```bash
# Windows 集成测试脚本
test_windows.ps1

# 1. 安装测试
& .\scripts\install.ps1

# 2. 运行功能测试
& cargo test --target x86_64-pc-windows-msvc

# 3. 运行性能测试
& .\scripts\benchmark.ps1

# 4. 硬件加速测试
& .\scripts\hwaccel_test.ps1
```

### 测试矩阵

| 平台 | 版本 | GPU | 硬件加速 | 软件解码 |
|------|------|-----|----------|----------|
| Windows | 10 22H2 | NVIDIA RTX 3060 | ✅ | ✅ |
| Windows | 10 22H2 | Intel Xe | ✅ | ✅ |
| Windows | 10 22H2 | AMD RX 6700 | ✅ | ✅ |
| Windows | 11 22H2 | NVIDIA RTX 4070 | ✅ | ✅ |
| macOS | 13 Ventura | Apple M2 | ✅ | ✅ |
| macOS | 14 Sonoma | Apple M3 Pro | ✅ | ✅ |
| macOS | 12 Monterey | Intel i7-11800H | ✅ | ✅ |
| Linux | Ubuntu 23.10 | NVIDIA | ✅ | ✅ |
| Linux | Fedora 39 | AMD | ✅ | ✅ |

---

## 风险与缓解

### 技术风险

| 风险 | 影响 | 可能性 | 缓解措施 |
|------|------|--------|----------|
| Media Foundation API 复杂 | 开发延期 | 中 | 渐进式实现，先完成软件解码 |
| VideoToolbox 参数错误 | 崩溃 | 中 | 添加错误处理和日志 |
| FFmpeg Windows 构建 | 编译失败 | 中 | 使用预编译静态库 |
| 硬件加速兼容性 | 功能降级 | 高 | 完善的软件解码回退 |
| Windows 权限问题 | 功能缺失 | 低 | 检测并提示用户 |

### 依赖风险

| 依赖 | 风险 | 缓解措施 |
|------|------|----------|
| windows-sys | API 变更 | 锁定 0.52 版本 |
| ffmpeg-next | 硬件加速支持不完整 | 测试 FFmpeg 7.0, 7.1 |
| wgpu | 后端不稳定 | 测试 Vulkan/DX12/Metal |

### 回退策略

```
硬件加速失败时:
1. 记录详细错误日志
2. 自动回退到软件解码
3. 在 UI 显示警告 (可关闭)
4. 提示用户更新驱动
```

---

## 时间线

### 详细计划

```
Week 1-2: Windows 支持 (Phase 1)
├─ Day 1-3: Media Foundation 解码器框架
├─ Day 4-7: 硬件加速实现
├─ Day 8-10: 软件解码回退
├─ Day 11-12: GPU 检测 (DXGI)
├─ Day 13-14: 进程管理 (windows-sys)
└─ Week 2 末: Windows 内部测试

Week 3-4: Windows 完善 + macOS 开始 (Phase 2)
├─ Day 15-18: Bug 修复 + 优化
├─ Day 19-21: VideoToolbox 解码器框架
├─ Day 22-24: 硬件加速实现
├─ Day 25-26: 软件解码回退
├─ Day 27-28: macOS GPU 检测 (IOKit)
└─ Week 4 末: macOS 内部测试

Week 5: 跨桌面环境 + 文档 (Phase 3)
├─ Day 29-31: Linux 桌面环境测试 (GNOME/Xfce/Sway)
├─ Day 32-33: VAAPI 路径动态枚举
├─ Day 34-35: 文档完善
└─ Week 5 末: 公共测试版发布
```

### 里程碑

| 日期 | 里程碑 | 交付物 |
|------|--------|--------|
| Week 2 末 | Windows 内部 Alpha | Windows 64-bit 构建 |
| Week 4 末 | macOS 内部 Alpha | macOS x86_64/arm64 构建 |
| Week 5 末 | 公共 Beta | 多平台 Beta 发布 |
| Week 6 末 | 正式发布 v0.2 | 完整跨平台支持 |

---

## 附录

### A. 相关文档

- [架构文档](ARCHITECTURE.md)
- [配置指南](configuration.md)
- [延迟优化](LATENCY_OPTIMIZATION.md)
- [开发陷阱](pitfalls.md)

### B. 参考实现

- [scrcpy](https://github.com/Genymobile/scrcpy) - 跨平台实现参考
- [FFmpeg HW Acceleration](https://trac.ffmpeg.org/wiki/HWAccelIntro) - 硬件加速文档
- [Media Foundation Samples](https://github.com/microsoft/Windows-classic-samples) - MS 官方示例

### C. 贡献指南

跨平台支持需要以下帮助:

- [ ] Windows 测试 (多种 GPU 配置)
- [ ] macOS 测试 (Intel + Apple Silicon)
- [ ] Linux 桌面环境测试 (GNOME/Xfce/Sway)
- [ ] 文档翻译 (中文/英文)

---

**最后更新**: 2026-01-21  
**维护者**: SAide Team
