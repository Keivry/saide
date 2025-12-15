
### 4. 音视频同步时音频流阻塞问题（2025-12-12）✅ 已解决

**现象**：
- 画面正常持续更新
- 音频线程启动后只读取 2 个包就永久阻塞
- 通知声音可以转发，但音乐播放器/媒体音无输出
- test_audio（纯音频模式）工作正常

**根因**：
**未读取音频流的 codec_id 导致协议错位**

scrcpy 协议规定每个流的第一个数据必须是 4 字节 codec_id：
- 视频流：`codec_id(4 bytes) + width(4) + height(4) + packets...`
- 音频流：`codec_id(4 bytes) + packets...`

我们的代码只读取了视频流的 codec metadata，完全跳过了音频流的 codec_id 读取。
导致音频线程尝试读取 12 字节包头（PTS 8 bytes + size 4 bytes）时，实际读到的是：
```
[codec_id: 4 bytes][packet_header 前 8 bytes]
```
协议完全错位，后续所有读取都失败。

**错误代码示例**：
```rust
// ❌ 错误：只读取视频流 codec metadata
let video_resolution = if params.send_codec_meta {
    stream.read_exact(&mut codec_meta[12])?; // 读取 codec_id + width + height
    ...
};

// ❌ 音频流直接跳过，没有读取 codec_id
let audio_stream = conn.audio_stream.take()?;
// 直接开始读取包头 → 错误！第一个 4 字节是 codec_id，不是包头

// 音频线程阻塞在这里
audio_stream.read_exact(&mut header[12])?; // 实际读到 codec_id + 部分包头
```

**正确修复**：
```rust
// ✅ 正确：音频流也必须先读取 codec_id
if params.send_codec_meta && let Some(ref mut stream) = audio_stream {
    let mut codec_id_bytes = [0u8; 4];
    stream.read_exact(&mut codec_id_bytes)?;
    let codec_id = u32::from_be_bytes(codec_id_bytes);
    debug!("Audio codec meta: id=0x{:08x}", codec_id); // 0x6f707573 = "opus"
    
    // 检查特殊值（参考 demuxer.c）
    if codec_id == 0 { /* 流被设备禁用 */ }
    if codec_id == 1 { /* 流配置错误 */ }
}
// 现在可以正常读取包头了
```

**诊断过程**：
1. ✅ 排除连接问题：video → audio → control 三个 socket 都正确建立
2. ✅ 排除 FD 传递：reverse 模式下不需要通过 control 传递 FD
3. ✅ 排除音频源配置：`output` 和 `playback` 模式都有相同问题
4. ✅ 对比 scrcpy 官方客户端：音视频正常工作
5. 🔍 **深度源码分析**：发现 `demuxer.c` 中 `run_demuxer()` 第一步就是读取 codec_id
6. 🎯 **定位根因**：我们的 `connection.rs` 只处理了视频流 codec metadata

**测试结果**（修复后）：
```
✅ 音频 codec 初始化：id=0x6f707573 (opus)
✅ 音频包统计：734 packets/15s 持续解码
✅ 音频处理进度：100 → 200 → ... → 700 packets
✅ 视频帧数：600+ 帧，0 丢帧
✅ 音视频同步流畅，无延迟
```

**关键教训**：
1. **协议必须完整实现**：即使某些字段看起来"可选"，也必须按协议顺序读取
2. **对比官方实现**：遇到问题时直接对比官方客户端行为，快速定位差异
3. **深度读源码**：关键协议细节往往隐藏在源码注释和边界条件处理中
4. **多流处理的对称性**：如果视频流需要读 codec_id，音频流也一定需要

**相关代码位置**：
- 修复：`src/scrcpy/connection.rs` line 185-203
- 参考：`3rd-party/scrcpy/app/src/demuxer.c` line 145-158 (`run_demuxer`)
- 协议定义：`demuxer.c` line 81-100（包头格式注释）


---

## 配置管理（新增）

### 6. config.toml 配置未生效（硬编码参数）(2025-12-15)

**问题现象**：
- 修改 `config.toml` 中的 `max_size = 720`
- 但 scrcpy 实际传参仍是 `max_size=1920`
- 所有 scrcpy 参数（bit_rate, codec 等）都未生效

**根本原因**：
- `StreamPlayer::start()` 内部调用 `stream_worker()` 创建连接参数
- `stream_worker()` 中硬编码了所有参数
- 配置根本没有传递到实际连接逻辑

**解决方案**：
1. 修改 `StreamPlayer::start()` 签名，接受 `ScrcpyConfig`
2. 在 `stream_worker()` 中解析配置（支持 "24M" 格式）
3. 为 `ScrcpyConfig` 及其子结构添加 `Clone` trait

**教训**：
- 配置驱动的系统必须端到端验证配置传递
- 警惕硬编码，特别是在多层函数调用中

---

## UI 渲染（新增）

### 7. 初始化时视频不显示，需鼠标移动触发 (2025-12-15)

**问题现象**：程序启动后画面黑屏，鼠标移动后突然显示

**根本原因**：
- `InitState::InProgress` 未调用 `ctx.request_repaint()`
- egui 默认只在交互事件时重绘

**解决方案**：
```rust
InitState::InProgress => {
    self.player.update();
    self.check_init_stage(ctx);
    ctx.request_repaint();  // 主动请求重绘
}
```

**教训**：异步初始化场景必须主动请求重绘

---

## 状态管理（新增）

### 8. 旋转按钮点击无反应（no-op 方法）(2025-12-15)

**问题现象**：点击旋转按钮，状态更新但视频不旋转

**根本原因**：`StreamPlayer::set_rotation()` 为空操作（遗留代码）

**解决方案**：
1. 添加 `video_rotation: u32` 字段
2. 实现真正的 setter：`self.video_rotation = rotation % 4;`
3. 后续需在渲染时应用旋转变换

**教训**：警惕 no-op 方法和"兼容性"代码

---

## Rust 类型系统（新增）

### 9. 类型嵌套层数错误（Arc<Arc<T>>）(2025-12-15)

**问题**：`Arc::new(config.scrcpy.clone())` 导致双层 Arc

**解决**：直接 clone：`(*config.scrcpy).clone()`

**教训**：理解返回值类型，避免无意义的包裹

---

### 10. 缺少 Clone trait (2025-12-15)

**问题**：配置结构未派生 `Clone`，无法跨线程传递

**解决**：为所有配置结构添加 `#[derive(Clone)]`

**教训**：配置数据结构通常需要 Clone

---

## 平台特定问题

### 11. Wayland ResizeIncrements 警告 (2025-12-15)

**现象**：在 Wayland 环境运行时出现警告
```
WARN winit::platform_impl::linux::wayland::window: 
  `set_resize_increments` is not implemented for Wayland
```

**原因**：
- `ResizeIncrements` 是 X11 特有功能，用于锁定窗口大小调整步进
- Wayland 协议不支持这个功能（设计哲学不同）
- winit 库在 Wayland 上调用时返回未实现警告

**影响**：
- ✅ 不影响窗口初始化和旋转功能
- ✅ 窗口仍可手动调整大小
- ❌ 无法在拖拽时强制锁定宽高比

**解决**：
无需修复，这是平台限制。用户在 Wayland 环境下手动调整窗口时可能破坏宽高比，但旋转和自动调整仍然正常工作。

**如需屏蔽警告**：
```bash
RUST_LOG=error cargo run  # 只显示 error 级别日志
```

---

## 硬件解码（新增）

### 12. NVDEC 旋转崩溃 — 不支持 prepend-sps-pps 的设备 (2025-12-15)

**问题现象**：
- VAAPI 和软件解码：旋转正常 ✅
- NVDEC + `prepend-sps-pps-to-idr-frames=1`：通过 SPS 检测分辨率，重建解码器正常 ✅
- NVDEC + 不支持该选项的 Android 设备：旋转后视频画面崩溃 ❌

**根本原因**：
1. 部分 Android 设备不支持 `prepend-sps-pps-to-idr-frames=1` 选项（如 HiSilicon Kirin 980）
2. 旋转导致视频分辨率变化（如 592x1280 → 1280x592）
3. NVDEC 硬件解码器内部上下文（AVHWFramesContext）与新分辨率不兼容
4. FFmpeg 错误：`AVHWFramesContext is already initialized with incompatible parameters`
5. 后续所有帧解码失败：`CUDA_ERROR_INVALID_HANDLE: invalid resource handle`
6. 无 SPS 数据，无法提前检测分辨率变化

**解决方案（三层防御）**：

**第一层：容忍 FFmpeg 错误**
```rust
// src/decoder/nvdec.rs
fn send_packet(&mut self, data: &[u8], pts: i64) -> Result<()> {
    if let Err(e) = self.decoder.send_packet(&packet) {
        warn!("send_packet failed (possibly resolution change): {:?}", e);
        // Don't fail - let empty frame detection handle it
    }
    Ok(())
}

fn receive_frames(&mut self) -> Result<Vec<DecodedFrame>> {
    match self.decoder.receive_frame(&mut hw_frame) {
        Err(e) => {
            warn!("receive_frame failed: {:?}", e);
            break; // Return empty frames
        }
    }
}
```

**第二层：空帧计数器**
```rust
// 连续 3 帧空帧触发错误
if frames.is_empty() {
    self.consecutive_empty_frames += 1;
    if self.consecutive_empty_frames >= 3 {
        bail!("NVDEC decoder stuck: {} consecutive empty frames", ...);
    }
}
```

**第三层：双策略恢复**
```rust
// stream_player.rs: try_recover_decoder()
// Strategy 1: Try to extract SPS from failed packet
if let Some((w, h)) = extract_resolution_from_stream(packet_data) {
    if w > 32 && h > 32 {  // Filter invalid init values
        AutoDecoder::new(w, h)  // ✅ 重建解码器
    }
}

// Strategy 2: No SPS? Assume screen rotation (swap dimensions)
let swapped = (last_height, last_width);  // 592x1280 -> 1280x592
AutoDecoder::new(swapped.0, swapped.1)    // ✅ 重建解码器
```

**实现细节**：
1. **错误容忍**：NVDEC 在遇到 CUDA 错误时不立即失败，返回空帧
2. **空帧检测**：连续 3 帧空帧触发 "consecutive empty frames" 错误
3. **策略 1**：尝试从失败的数据包中提取 SPS（即使不是 Key 帧）
4. **策略 2**：如无 SPS，交换宽高（Android 旋转最常见场景：90°/270°）
5. **尺寸过滤**：忽略 32x32 等明显无效的分辨率（编码器初始化伪值）

**调试日志示例**：
```
旋转前：
2025-12-15T06:06:33.263194Z  INFO Video resolution: 592x1280
2025-12-15T06:06:33.410326Z DEBUG NVDEC H.264 decoder initialized

旋转时（FFmpeg错误）：
[h264_cuvid @ ...] AVHWFramesContext is already initialized with incompatible parameters
[h264_cuvid @ ...] CUDA_ERROR_INVALID_HANDLE: invalid resource handle
（重复多次）

旋转后（预期日志）：
2025-12-15T06:06:43.253884Z DEBUG Rotation changed: Some(0) -> 1
2025-12-15T06:06:43.262214Z DEBUG Device rotated to orientation: 90
2025-12-15T06:06:44.XXX  WARN ⚠️ NVDEC detected resolution change via decode failure
2025-12-15T06:06:44.XXX  INFO 🔄 No SPS found, trying dimension swap: 592x1280 -> 1280x592
2025-12-15T06:06:44.XXX  INFO ✅ Decoder recreated with swapped dimensions: NVDEC
```

**教训**：
- **FFmpeg 硬件解码脆弱性**：分辨率变化时 AVHWFramesContext 不能热更新
- **错误传播链**：CUDA 错误 → FFmpeg 错误 → 需要 Rust 层容忍
- **Android 碎片化**：同一 MediaCodec 参数在不同设备支持度差异大（HiSilicon vs MTK）
- **多层防御**：容忍 + 检测 + 恢复，缺一不可
- **延迟 vs 兼容性**：`prepend-sps-pps=1` 可优化但非必须，代码需容忍缺失

**相关代码**：
- `src/app/ui/stream_player.rs` line 46-110 (`try_recover_decoder`)
- `src/decoder/nvdec.rs` line 112-130 (error tolerance)
- `src/decoder/nvdec.rs` line 216-228 (empty frame detection)

**测试建议**：
```bash
# 使用不支持 prepend-sps-pps 的设备测试（如 HiSilicon Kirin 980）
cargo run
# 1. 等待视频正常显示
# 2. 旋转设备屏幕（90°或270°）
# 3. 观察日志：
#    - [h264_cuvid] CUDA_ERROR_INVALID_HANDLE (FFmpeg错误)
#    - ⚠️ NVDEC detected resolution change via decode failure
#    - 🔄 No SPS found, trying dimension swap: 592x1280 -> 1280x592
#    - ✅ Decoder recreated with swapped dimensions: NVDEC
# 4. 确认视频在 ~1 秒内恢复正常
```

**防护机制 - 防止重建循环**：
```rust
// 使用 Option 跟踪重建时间（首次重建允许，后续需冷却期）
let mut last_decoder_rebuild: Option<Instant> = None;

if let Some(last_rebuild_time) = last_decoder_rebuild {
    // 冷却期：2 秒或 10 帧
    const MIN_REBUILD_INTERVAL: Duration = Duration::from_secs(2);
    const MIN_FRAMES_BEFORE_REBUILD: u32 = 10;
    
    let can_rebuild = elapsed >= MIN_REBUILD_INTERVAL 
        || frames >= MIN_FRAMES_BEFORE_REBUILD;
    
    if !can_rebuild {
        continue;  // 跳过重建
    }
}

// 重建后更新时间
last_decoder_rebuild = Some(Instant::now());
```

**关键设计**：
- **首次重建无限制**：`None` 状态允许立即重建
- **后续重建需冷却**：`Some(time)` 状态强制 2 秒或 10 帧间隔
- **原因**: 新解码器需要时间接收正确分辨率的帧，否则立即又会触发"连续 3 帧空帧"

**最终解决方案（2025-12-15）**：

## 完美方案：capture_orientation

**策略**：使用 NVDEC 时**总是锁定 capture_orientation**

### 实现

```rust
// 检测到 NVIDIA GPU → 自动锁定方向
if has_nvidia_gpu() {
    params.capture_orientation = Some("@0".to_string());
}
```

### 原理

- scrcpy-server 以固定方向抓取屏幕
- 视频分辨率永不变化（592x1280 或 1280x592）
- NVDEC 解码器无需重建
- 系统自动旋转显示内容

### 优势

1. ✅ **根本性解决**：阻止分辨率变化，而不是处理变化
2. ✅ **零性能损失**：无解码器重建（~200ms 开销）
3. ✅ **无黑屏**：解码持续进行
4. ✅ **不需要 SPS**：无需 `prepend-sps-pps-to-idr-frames=1`
5. ✅ **通用性强**：所有 NVDEC 设备都受益
6. ✅ **兼容性好**：scrcpy 原生支持

### 为什么不用 prepend-sps-pps？

| 方案 | 优势 | 劣势 |
|------|------|------|
| prepend-sps + 重建 | 支持旋转 | 兼容性差、有重建开销、可能黑屏 |
| **capture_orientation** | **通用、稳定、零开销** | **方向固定（可接受）** |

### 受影响设备

- **所有使用 NVDEC 的设备**（自动检测 NVIDIA GPU）
- 视频方向固定，但始终正常工作
- 用户可通过配置禁用此行为

### 回退方案

如果用户需要视频随设备旋转：
1. 使用 VAAPI 解码器（Intel GPU）
2. 使用软件解码器
3. 手动禁用 `capture_orientation` 锁定

### 代码位置

- `src/scrcpy/server.rs`: `should_lock_orientation_for_nvdec()`
- `src/app/init.rs`: 自动应用逻辑

---

**最后更新**: 2025-12-15
