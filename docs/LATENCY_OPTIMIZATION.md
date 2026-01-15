# SAide 延迟优化路线图

> **创建时间**: 2026-01-14  
> **状态**: 规划阶段  
> **预期总收益**: 30-80ms 端到端延迟降低

---

## 目录

- [背景](#背景)
- [当前架构分析](#当前架构分析)
- [优化方案](#优化方案)
  - [1. 视频解码延迟优化](#1-视频解码延迟优化)
  - [2. 音频播放延迟优化](#2-音频播放延迟优化)
  - [3. AV 同步策略优化](#3-av-同步策略优化)
  - [4. 网络传输优化](#4-网络传输优化)
  - [5. 输入延迟优化](#5-输入延迟优化)
- [实施路线图](#实施路线图)
- [风险评估](#风险评估)
- [延迟测量系统](#延迟测量系统)

---

## 背景

SAide 作为 scrcpy 伴侣应用,核心目标是实现超低延迟的 Android 设备镜像和控制。当前架构已采用多项低延迟设计:

- 单帧缓冲 (FRAME_BUFFER_SIZE = 1)
- TCP_NODELAY 网络优化
- Lock-free 音频环形缓冲
- 硬件加速解码 (NVDEC/VAAPI)

但仍有 **30-80ms** 的优化空间。

---

## 当前架构分析

### 视频管道

```
Android 设备捕获 → H264编码 → TCP → 本地接收 → 解码器 → 颜色转换 → GPU上传 → 渲染
   (Device)                    (1-5ms)    (5-15ms)    (2-5ms)      (2-5ms)   (1-3ms)
```

**当前瓶颈**:
- FFmpeg 软件颜色转换 (YUV420P → RGBA)
- 解码器内部缓冲
- CPU → GPU 内存拷贝

### 音频管道

```
Android 设备捕获 → Opus编码 → TCP → 本地接收 → Opus解码 → CPAL播放
   (Device)                    (1-3ms)    (1-2ms)      (2-8ms)
```

**当前瓶颈**:
- CPAL 共享模式额外缓冲
- 音频缓冲区大小 (128 frames = 2.67ms)

### 输入管道

```
用户输入 → egui事件 → 坐标映射 → 控制消息序列化 → TCP发送 → Android执行
         (1-3ms)      (<1ms)       (<1ms)            (1-5ms)
```

**当前瓶颈**:
- egui 事件循环延迟
- 每个消息都立即 flush

---

## 优化方案

### 1. 视频解码延迟优化

#### 1.1 消除解码器内部缓冲 ⭐⭐⭐⭐⭐

**问题**: FFmpeg 解码器默认会缓冲多帧用于 B 帧重排序

**方案**:
```rust
// 位置: src/decoder/h264.rs:59
// 当前:
(*ctx_ptr).flags |= ffmpeg::sys::AV_CODEC_FLAG_LOW_DELAY as i32;

// 优化:
(*ctx_ptr).flags |= ffmpeg::sys::AV_CODEC_FLAG_LOW_DELAY as i32;
(*ctx_ptr).flags2 |= ffmpeg::sys::AV_CODEC_FLAG2_FAST as i32;
(*ctx_ptr).strict_std_compliance = ffmpeg::sys::FF_COMPLIANCE_EXPERIMENTAL;
```

**收益**: 5-10ms  
**风险**: 低 (scrcpy 已禁用 B 帧)  
**优先级**: P0

---

#### 1.2 零拷贝 GPU 解码 ⭐⭐⭐⭐

**问题**: 当前流程 `解码 → CPU内存 → ScalerContext → GPU上传`,涉及多次内存拷贝

**方案**:
```rust
// 位置: src/decoder/nvdec.rs, vaapi.rs
// 优化目标: 解码器直接输出 GPU 纹理

// NVDEC 方案:
// 1. 使用 AV_PIX_FMT_CUDA 输出格式
// 2. 直接映射 CUDA 纹理到 WGPU
// 3. 在着色器中完成 YUV → RGBA 转换

// VAAPI 方案:
// 1. 使用 AV_PIX_FMT_VAAPI 输出格式
// 2. 导出 VA Surface 为 DMA-BUF
// 3. 导入 DMA-BUF 到 WGPU (Vulkan external memory)
```

**收益**: 10-15ms  
**风险**: 中 (需要修改渲染管线)  
**优先级**: P1

---

### 2. 音频播放延迟优化

#### 2.1 减少音频缓冲区 ⭐⭐⭐

**问题**: 当前 128 frames (2.67ms) 缓冲,部分系统可支持更小

**方案**:
```rust
// 位置: src/constant.rs:46
// 当前:
pub const AUDIO_BUFFER_FRAMES: usize = 128;

// 优化 (需测试硬件支持):
pub const AUDIO_BUFFER_FRAMES: usize = 64;  // 1.33ms @ 48kHz
```

**收益**: 1-2ms  
**风险**: 中 (可能导致 underrun)  
**优先级**: P2

---

#### 2.2 CPAL 独占模式 ⭐⭐⭐⭐

**问题**: 共享模式下系统可能添加额外混音缓冲

**方案**:
```rust
// 位置: src/decoder/audio/player.rs:91
// 当前使用默认共享模式

// 优化: 尝试独占模式 (Linux/Windows)
#[cfg(any(target_os = "linux", target_os = "windows"))]
{
    let stream = device.build_output_stream_raw(
        &config,
        SampleFormat::F32,
        move |data, _| { audio_callback(...) },
        |err| { warn!("Audio error: {}", err); },
        Some(Duration::ZERO), // 独占模式
    )?;
}
```

**收益**: 3-8ms  
**风险**: 中 (部分系统不支持)  
**优先级**: P1

---

### 3. AV 同步策略优化

#### 3.1 动态同步阈值 ⭐⭐⭐

**问题**: 固定 20ms 阈值可能在网络抖动时过于激进

**方案**:
```rust
// 位置: src/avsync/clock.rs
// 新增抖动估算器

pub struct JitterEstimator {
    samples: VecDeque<i64>,
    window_size: usize,
}

impl JitterEstimator {
    pub fn estimate(&self) -> i64 {
        // 计算 95 百分位延迟
        let mut sorted = self.samples.iter().copied().collect::<Vec<_>>();
        sorted.sort_unstable();
        sorted[self.samples.len() * 95 / 100]
    }
}

impl AVSync {
    pub fn update_audio_pts(&mut self, audio_pts: i64) {
        // ... 现有逻辑 ...
        
        // 动态调整阈值
        self.jitter_estimator.update(audio_pts);
        self.snapshot.threshold_us = self.jitter_estimator.estimate() * 2;
    }
}
```

**收益**: 3-5ms (减少不必要丢帧)  
**风险**: 低  
**优先级**: P1

---

#### 3.2 可配置禁用 AV 同步 ⭐⭐

**问题**: 超低延迟场景可能愿意牺牲音画同步

**方案**:
```rust
// 位置: src/config/scrcpy.rs
pub struct ScrcpyConfig {
    // ... 现有字段 ...
    
    /// 禁用 AV 同步 (优先延迟,可能导致音画不同步)
    pub disable_av_sync: bool,
}

// 位置: src/saide/ui/player.rs:686
if !config.disable_av_sync && av_snapshot.should_drop_video(frame.pts) {
    // 丢帧逻辑
}
```

**收益**: 5-10ms  
**风险**: 高 (音画可能不同步)  
**优先级**: P3 (可选配置)

---

### 4. 网络传输优化

#### 4.1 TCP 快速确认 ⭐⭐⭐⭐⭐

**问题**: 默认 TCP 可能延迟 ACK 发送

**方案**:
```rust
// 位置: src/scrcpy/connection.rs:433
// 在 TCP_NODELAY 之后添加:

#[cfg(target_os = "linux")]
{
    use std::os::unix::io::AsRawFd;
    use nix::sys::socket::{setsockopt, sockopt};
    
    let fd = stream.as_raw_fd();
    
    // TCP_QUICKACK: 立即发送 ACK
    if let Err(e) = setsockopt(fd, sockopt::TcpQuickAck, &true) {
        warn!("Failed to set TCP_QUICKACK: {}", e);
    }
    
    debug!("{} connection: TCP_QUICKACK enabled", channel);
}
```

**收益**: 3-5ms  
**风险**: 低 (仅 Linux)  
**优先级**: P0

---

#### 4.2 UDP 替代方案 ⭐⭐ (重构级)

**问题**: TCP 可靠性机制引入延迟 (重传、拥塞控制)

**方案**:
```rust
// 位置: 新建 src/scrcpy/udp_transport.rs
// 视频流使用 UDP + 前向纠错 (FEC)
// 音频/控制仍用 TCP

// 优点: 无重传延迟,丢包时降低质量而非卡顿
// 缺点: 需要 scrcpy-server 配合,实施复杂度极高
```

**收益**: 10-20ms (在丢包场景)  
**风险**: 极高 (需协议重构)  
**优先级**: P4 (长期研究)

---

### 5. 输入延迟优化

#### 5.1 消除过度 flush ⭐⭐⭐⭐⭐

**问题**: 每个控制消息都立即 flush,增加系统调用开销

**方案**:
```rust
// 位置: src/controller/control_sender.rs:52
// 当前:
let mut stream = self.stream.lock();
stream.write_all(&buf)?;
stream.flush()?;  // ← 移除

// 优化: 依赖 TCP_NODELAY 自动推送
let mut stream = self.stream.lock();
stream.write_all(&buf)?;
// 让 TCP 栈决定何时发送 (通常立即发送小包)
```

**收益**: 2-5ms  
**风险**: 低  
**优先级**: P0

---

#### 5.2 原始输入设备监听 ⭐⭐⭐ (平台相关)

**问题**: egui 事件循环可能有额外延迟 (约 1-3ms)

**方案**:
```rust
// 位置: 新建 src/input/raw_device.rs
// Linux: evdev 直接读取 /dev/input/event*
// Windows: Raw Input API
// macOS: IOKit

#[cfg(target_os = "linux")]
use evdev::{Device, InputEventKind};

pub fn spawn_raw_input_thread(sender: ControlSender) -> JoinHandle<()> {
    thread::spawn(move || {
        let mut device = Device::open("/dev/input/event0").unwrap();
        for event in device.fetch_events().unwrap() {
            if let InputEventKind::Key(key) = event.kind() {
                // 直接发送,绕过 UI 事件循环
                sender.send_raw_key(key);
            }
        }
    })
}
```

**收益**: 3-10ms  
**风险**: 中 (需要额外权限,平台相关)  
**优先级**: P2

---

#### 5.3 鼠标移动速度自适应 ⭐⭐⭐

**问题**: 固定 8ms 间隔,快速移动时可能丢失细节

**方案**:
```rust
// 位置: src/controller/mouse.rs:95
const MIN_DRAG_INTERVAL_MS: u128 = 4;  // 快速移动
const MAX_DRAG_INTERVAL_MS: u128 = 16; // 慢速移动

impl MouseMapper {
    fn update(&self) -> Result<()> {
        if let MouseState::Dragging { current_x, current_y, last_update, .. } = state {
            let distance = calculate_distance(prev_x, prev_y, current_x, current_y);
            let elapsed = last_update.elapsed().as_millis();
            let speed = distance / elapsed as f32;
            
            let interval = if speed > 100.0 {
                MIN_DRAG_INTERVAL_MS
            } else if speed < 10.0 {
                MAX_DRAG_INTERVAL_MS
            } else {
                8
            };
            
            if elapsed >= interval {
                self.sender.send_touch_move(current_x, current_y)?;
            }
        }
    }
}
```

**收益**: 2-5ms (减少不必要的消息)  
**风险**: 低  
**优先级**: P2

---

## 实施路线图

### 第 1 周: 基础设施 + 阶段一优化

**目标**: 建立延迟测量系统,实施低风险优化

- [ ] 实现 `LatencyProfiler` (src/profiler/latency.rs)
- [ ] 添加延迟统计到 UI 指示器
- [ ] 启用 `AV_CODEC_FLAG2_FAST` (h264.rs)
- [ ] Linux TCP_QUICKACK 优化 (connection.rs)
- [ ] 移除 `ControlSender` 过度 flush (control_sender.rs)
- [ ] 动态 AV 同步阈值 (avsync/clock.rs)

**预期收益**: 10-20ms  
**文档**: 更新 `docs/LATENCY_OPTIMIZATION.md` 记录基准测试结果

---

### 第 2-3 周: 阶段二优化 (零拷贝 GPU)

**目标**: 实现零拷贝 GPU 解码

- [ ] NVDEC 零拷贝路径 (decoder/nvdec.rs)
  - 输出 AV_PIX_FMT_CUDA
  - CUDA → WGPU 互操作
  - GPU 着色器 YUV→RGBA 转换
- [ ] VAAPI 零拷贝路径 (decoder/vaapi.rs)
  - 输出 AV_PIX_FMT_VAAPI
  - DMA-BUF 导出/导入
  - GPU 着色器 YUV→RGBA 转换
- [ ] 更新 NV12 渲染器适配新格式 (decoder/nv12_render.rs)

**预期收益**: 20-40ms (累计)  
**风险**: 中 (需要测试多种 GPU)  
**文档**: 更新 `docs/ARCHITECTURE.md` 添加零拷贝流程图

---

### 第 4 周: 阶段三优化 (音频 + 输入)

**目标**: 优化音频播放和输入响应

- [ ] CPAL 独占模式尝试 (decoder/audio/player.rs)
- [ ] 音频缓冲降至 64 frames (constant.rs)
- [ ] 原始输入设备监听 (新建 input/raw_device.rs)
  - Linux evdev 实现
  - Windows Raw Input (可选)
- [ ] 鼠标移动速度自适应 (controller/mouse.rs)

**预期收益**: 30-60ms (累计)  
**文档**: 更新 `docs/pitfalls.md` 记录音频独占模式兼容性

---

### 后续: 长期研究

- [ ] UDP 视频传输协议调研
- [ ] 可配置禁用 AV 同步 (scrcpy.rs config)
- [ ] 自定义低延迟视频编解码器 (AV1 SVC)

---

## 风险评估

| 优化方案 | 延迟收益 | 实施难度 | 稳定性风险 | 优先级 |
|---------|---------|---------|-----------|--------|
| 消除解码器缓冲 | 5-10ms | 低 | 低 | P0 ⭐⭐⭐⭐⭐ |
| 零拷贝 GPU 解码 | 10-15ms | 高 | 中 | P1 ⭐⭐⭐⭐ |
| TCP_QUICKACK | 3-5ms | 低 | 低 | P0 ⭐⭐⭐⭐⭐ |
| 移除过度 flush | 2-5ms | 低 | 低 | P0 ⭐⭐⭐⭐⭐ |
| CPAL 独占模式 | 3-8ms | 中 | 中 | P1 ⭐⭐⭐⭐ |
| 原始输入设备 | 5-10ms | 高 | 中 | P2 ⭐⭐⭐ |
| UDP 视频流 | 10-20ms | 极高 | 高 | P4 ⭐⭐ |
| 禁用 AV 同步 | 5-10ms | 低 | 高 | P3 ⭐⭐ |

**图例**:
- **延迟收益**: 预期端到端延迟降低
- **实施难度**: 代码修改量 + 测试复杂度
- **稳定性风险**: 引入 bug 或兼容性问题的可能性
- **优先级**: P0 最高, P4 最低

---

## 延迟测量系统

为确保优化有效,需建立完整的延迟测量系统:

### 架构

```rust
// 位置: 新建 src/profiler/latency.rs

use std::time::Instant;

/// 延迟分析器 - 追踪各阶段时间戳
pub struct LatencyProfiler {
    /// 设备捕获时间 (从 PTS 推算)
    pub capture_time: Option<Instant>,
    
    /// TCP 接收时间 (第一个字节到达)
    pub receive_time: Option<Instant>,
    
    /// 解码完成时间
    pub decode_time: Option<Instant>,
    
    /// GPU 上传完成时间
    pub upload_time: Option<Instant>,
    
    /// 渲染到屏幕时间 (vsync)
    pub display_time: Option<Instant>,
}

impl LatencyProfiler {
    /// 计算端到端延迟
    pub fn end_to_end_latency(&self) -> Option<Duration> {
        Some(self.display_time?.duration_since(self.capture_time?))
    }
    
    /// 各阶段延迟分解
    pub fn breakdown(&self) -> LatencyBreakdown {
        LatencyBreakdown {
            network: self.receive_time? - self.capture_time?,
            decode: self.decode_time? - self.receive_time?,
            upload: self.upload_time? - self.decode_time?,
            render: self.display_time? - self.upload_time?,
        }
    }
    
    /// 生成报告
    pub fn report(&self) {
        if let Some(breakdown) = self.breakdown() {
            info!("=== Latency Breakdown ===");
            info!("  网络传输: {:4.1}ms", breakdown.network.as_secs_f64() * 1000.0);
            info!("  视频解码: {:4.1}ms", breakdown.decode.as_secs_f64() * 1000.0);
            info!("  GPU上传 : {:4.1}ms", breakdown.upload.as_secs_f64() * 1000.0);
            info!("  渲染显示: {:4.1}ms", breakdown.render.as_secs_f64() * 1000.0);
            info!("  总延迟  : {:4.1}ms", self.end_to_end_latency()?.as_secs_f64() * 1000.0);
        }
    }
}

pub struct LatencyBreakdown {
    pub network: Duration,
    pub decode: Duration,
    pub upload: Duration,
    pub render: Duration,
}
```

### 集成点

```rust
// 位置: src/saide/ui/player.rs:640
let pts = video_packet.pts_us as i64;

// 新增: 初始化 profiler
let mut profiler = LatencyProfiler::default();
profiler.capture_time = Some(clock.pts_to_system_time(pts));
profiler.receive_time = Some(Instant::now());

// 解码后
let frame = video_decoder.decode(&video_packet.data, pts)?;
profiler.decode_time = Some(Instant::now());

// 上传后 (在 GPU 回调中)
profiler.upload_time = Some(Instant::now());

// 渲染后 (在 vsync 回调中)
profiler.display_time = Some(Instant::now());

// 每 60 帧输出一次报告
if frame_count % 60 == 0 {
    profiler.report();
}
```

### UI 展示

```rust
// 位置: src/saide/ui/indicator.rs
// 在 FPS 旁边显示延迟

ui.label(format!(
    "FPS: {:.1} | Latency: {:.1}ms",
    fps,
    latency_ms
));
```

---

## 已知限制 (Phase 1 实现)

### GPU 上传时间测量不精确

**现象**: UI Indicator 显示的 "GPU Upload" 时间为近似值,非实际 GPU 上传耗时

**原因**:
- `LatencyProfiler.mark_upload()` 在视频解码线程调用 (位于 `src/saide/ui/player.rs:703`)
- 实际 GPU 上传发生在 UI 渲染线程的 `Nv12RenderCallback::prepare()` 中 (位于 `src/decoder/nv12_render.rs:330`)
- 跨线程时间测量需要额外同步机制 (Phase 1 未实现)

**当前测量值含义**:
- 测量的是 "解码完成 → 帧发送到渲染通道" 的时间
- 包含通道发送延迟,但不包含实际 GPU texture 上传时间

**影响范围**:
- 平均延迟 (Avg) 和 P95 延迟: ✅ 准确 (端到端测量不受影响)
- 解码时间 (Decode): ✅ 准确 (Phase 1 已修复)
- GPU 上传时间 (Upload): ⚠️ 近似值 (误差约 1-3ms)

**Phase 2 改进计划**:
1. 在 `Arc<DecodedFrame>` 中嵌入 `Arc<Mutex<LatencyProfiler>>`
2. 在 `Nv12RenderCallback::prepare()` 开始和结束时分别调用 `mark_upload_start()` / `mark_upload_end()`
3. 通过另一个 channel 将精确上传时间回传到解码线程进行统计

**当前使用建议**:
- 使用平均延迟和 P95 延迟进行优化效果评估 (这两个值是准确的)
- GPU 上传时间仅用于相对比较,不要依赖其绝对值

---

## 成功指标

优化完成后,目标延迟指标:

| 场景 | 当前延迟 | 目标延迟 | 改进 |
|-----|---------|---------|-----|
| USB 连接 (理想) | 50-80ms | 20-40ms | -60% |
| WiFi 连接 (良好) | 80-120ms | 50-80ms | -37% |
| 输入响应 | 10-20ms | 5-10ms | -50% |

**测量方法**: 使用高速摄像机 (240fps) 拍摄设备屏幕和镜像窗口,计算帧间延迟

---

## 参考资料

- [scrcpy 官方延迟优化文档](https://github.com/Genymobile/scrcpy/blob/master/doc/video.md)
- [FFmpeg 低延迟编解码最佳实践](https://trac.ffmpeg.org/wiki/StreamingGuide)
- [CPAL 低延迟音频配置](https://github.com/RustAudio/cpal/wiki/Low-Latency-Guide)
- [Linux TCP 优化参数](https://www.kernel.org/doc/Documentation/networking/ip-sysctl.txt)

---

**维护者**: SAide Development Team  
**最后更新**: 2026-01-15
