
### 5. 程序退出时 CUDA 清理错误（2025-12-16）✅ 已解决

**现象**：
```
DEBUG saide::scrcpy::connection: Force killing server process (timeout)
WARN saide::app::ui::stream_player: Audio/Video read error - skipping
ERROR Connection error: Failed to read pts_and_flags
[AVHWDeviceContext @ 0x...] cu->cuMemFree() failed
[AVHWDeviceContext @ 0x...] cu->cuCtxDestroy() failed
Error: nu::shell::terminated_by_signal
```

**根因**：
**线程退出顺序不当 + 解码器资源未及时释放**

1. **StreamPlayer.stop()**：使用 `thread.is_finished()` 方法（需要 nightly Rust），导致运行时 panic
2. **ScrcpyConnection.shutdown()**：服务器进程等待超时（1s）后强制 `kill()`
3. **NVDEC Drop**：CUDA context 未 flush，直接 `av_buffer_unref()` 导致 CUDA 错误

**Rust 线程优雅退出的最佳实践**：

ChatGPT 推荐的方案（本次采用）：
1. ✅ **使用 `Arc<AtomicBool>` 作为退出信号**
2. ✅ **Channel 关闭触发线程退出**
3. ✅ **显式 `Drop` 顺序控制**（解码器 → socket → 服务器进程）
4. ✅ **超时 join 机制**（轮询 + `std::mem::forget` 替代 `thread.is_finished()`）

**修复方案**：

```rust
// 1. StreamPlayer: 安全的超时 join（不依赖 nightly）
pub fn stop(&mut self) {
    // 1. Drop channels first (signal threads to exit)
    self.frame_rx = None;
    self.stats_rx = None;

    // 2. Blocking join with timeout
    if let Some(thread) = self.stream_thread.take() {
        const JOIN_TIMEOUT_MS: u64 = 2000;
        let start = std::time::Instant::now();

        loop {
            if thread.is_finished() {
                let _ = thread.join();
                break;
            }
            if start.elapsed().as_millis() > JOIN_TIMEOUT_MS as u128 {
                warn!("Thread timeout, abandoning join");
                std::mem::forget(thread); // Detach thread (safe)
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
}

// 2. ScrcpyConnection: 延长等待时间 + 正确的退出顺序
pub fn shutdown(&mut self) -> Result<()> {
    // Step 1: Close sockets FIRST (triggers server exit via broken pipe)
    self.video_stream.take();
    self.audio_stream.take();
    self.control_stream.take();

    // Step 2: Wait longer for graceful exit (3s instead of 1s)
    if let Some(mut process) = self.server_process.take() {
        const MAX_WAIT_MS: u64 = 3000;
        let start = std::time::Instant::now();

        while start.elapsed().as_millis() < MAX_WAIT_MS as u128 {
            if let Ok(Some(status)) = process.try_wait() {
                debug!("Server exited gracefully: {:?}", status);
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        // Only kill if truly stuck
        debug!("Server timeout, force terminating");
        process.kill().ok();
    }

    // Step 3: Remove tunnel (safe to do last)
    remove_reverse_tunnel(...).ok();
    Ok(())
}

// 3. NVDEC Drop: 显式 flush + 延迟释放
impl Drop for NvdecDecoder {
    fn drop(&mut self) {
        unsafe {
            // 1. Flush decoder to release pending frames
            let _ = self.flush();

            // 2. Give CUDA time to finish async operations
            std::thread::sleep(std::time::Duration::from_millis(50));

            // 3. Release hardware device context
            if !self.hw_device_ctx.is_null() {
                ffmpeg::sys::av_buffer_unref(&mut self.hw_device_ctx);
            }
        }
    }
}

// 4. 显式 Drop 顺序（在 decode loop 中）
let decode_result = (|| -> Result<()> {
    loop {
        // ... decode logic ...

        if frame_tx.is_disconnected() {
            debug!("Channel disconnected, exiting gracefully");
            return Ok(()); // Drop will handle cleanup
        }
    }
})();

// Explicit drop BEFORE sockets close
drop(video_decoder);
debug!("Decoder dropped");
```

**关键改进点**：
1. ✅ **Arc<AtomicBool> 作为退出信号**：UI 销毁时 `stop_signal.store(true)` → 解码线程每次 read 前检查 → 立即退出
2. ✅ **解码器先于 socket 释放**：确保 CUDA 清理完成后再关闭网络连接
3. ✅ **服务器进程等待时间延长**：从 1s 增加到 3s，减少强制 kill 概率
4. ✅ **AudioPlayer Drop 延迟**：给音频回调 50ms 时间完成当前操作
5. ✅ **Loop 开头检查退出信号**：避免阻塞在 socket read（timeout 5s）无法响应

**后续补充修复（2025-12-16 实测）**：

**问题**：初版修复后仍然出现超时 kill：
```
INFO Stopping stream
WARN Stream thread did not finish within 2000ms, abandoning join
...（2秒后解码线程仍在工作）
DEBUG Server process timeout (3000ms), force terminating
```

**根因**：解码线程阻塞在 `VideoPacket::read_from()`（blocking read，timeout 5s），在此期间无法检测退出信号。

**最终方案**：
```rust
// StreamPlayer 结构体添加退出信号
struct StreamPlayer {
    stop_signal: Option<Arc<AtomicBool>>,
    // ...
}

// stop() 时设置信号
pub fn stop(&mut self) {
    if let Some(ref signal) = self.stop_signal {
        signal.store(true, Ordering::Relaxed);
    }
    self.frame_rx = None; // 释放 channel
    // ... join with timeout ...
}

// Worker 线程在 loop 开头检查
loop {
    // ✅ 关键：在 blocking read 之前检查
    if stop_signal.load(Ordering::Relaxed) {
        debug!("Stop signal received, stopping decode loop");
        return Ok(());
    }

    // 这里可能阻塞 5s（read timeout）
    let packet = VideoPacket::read_from(&mut video_stream)?;
    // ...
}
```

**为什么不能用 Channel**：
- `crossbeam_channel::Sender` 没有 `is_disconnected()` 方法
- 只能通过 `try_send()` 失败来判断，但需要实际发送数据才能检测
- `AtomicBool` 更轻量、响应更快

**参考 ChatGPT 回答的其他方案（未采用）**：
- `tokio::sync::Notify`：需要 async runtime，我们是同步代码
- `Condvar`：适合长时间阻塞的场景，我们已经有 read timeout
- `Worker` struct with `Drop`：已经在 `StreamPlayer` 中实现

**第三版优化（2025-12-16）- UI 响应速度**：

**问题**：UI 退出速度慢（3-5 秒）
- StreamPlayer.stop() 阻塞 2s 等待 join
- ScrcpyConnection.shutdown() 阻塞 3s 等待服务器退出

**用户体验目标**：点击关闭按钮 → UI 立即消失（<500ms）

**方案 3.0**：纯 Detached cleanup（失败）
```rust
pub fn stop(&mut self) {
    signal.store(true);
    self.frame_rx = None;
    
    std::thread::spawn(move || {
        let _ = thread.join(); // 完全后台
    });
    // 立即返回 ⚡
}
```
❌ **问题**：主线程退出 → CUDA driver cleanup → 后台线程 drop 解码器 → CUDA context 已失效
```
[AVHWDeviceContext @ ...] cu->cuCtxPushCurrent() failed
```

**最终方案 3.1**：Hybrid - 短暂阻塞 + Detached
```rust
// StreamPlayer: 立即返回，后台 join
pub fn stop(&mut self) {
    signal.store(true);
    self.frame_rx = None;
    
    if let Some(thread) = self.stream_thread.take() {
        std::thread::spawn(move || {
            // 后台等待 2s 或 join
            let _ = thread.join();
        });
    }
    // 立即返回，UI 不阻塞 ⚡
}

// ScrcpyConnection: 快速检查 + 后台清理
pub fn shutdown(&mut self) -> Result<()> {
    self.video_stream.take(); // 关闭 socket
    
    if let Some(mut process) = self.server_process.take() {
        // Fast path: 50ms 快速检查
        for _ in 0..5 {
            if process.try_wait().is_ok() {
                return Ok(()); // 快速退出 ⚡
            }
            sleep(10ms);
        }
        
        // Slow path: 后台清理（不阻塞 UI）
        std::thread::spawn(move || {
            // 最多等 3s 或 kill
        });
    }
    Ok(()) // 立即返回 ⚡
}
```

```rust
// Hybrid cleanup: 确保 CUDA 资源正确释放 + UI 快速响应
pub fn stop(&mut self) {
    signal.store(true);
    self.frame_rx = None;
    
    if let Some(thread) = self.stream_thread.take() {
        // Phase 1: 短暂阻塞等待解码器 drop (300ms)
        const DECODER_CLEANUP_MS: u64 = 300;
        for _ in 0..30 {
            if thread.is_finished() {
                let _ = thread.join(); // ✅ 解码器已 drop
                return; // 快速退出
            }
            sleep(10ms);
        }
        
        // Phase 2: 超时后 detach 剩余清理（网络/进程）
        std::thread::spawn(move || {
            let _ = thread.join(); // 后台完成
        });
    }
    // 300ms 内返回（90% 情况）或立即返回 ⚡
}
```

**性能对比**：
| 操作 | 修复前 | 方案 3.0（失败）| 方案 3.1（最终）|
|------|-------|--------------|--------------|
| **stop() 调用** | 阻塞 2s | 立即返回 <1ms | **等待 300ms（快速路径）** |
| **shutdown() 调用** | 阻塞 3s | 快速检查 50ms | 快速检查 50ms |
| **UI 关闭延迟** | **5s** | <100ms ❌ CUDA 错误 | **<400ms** ✅ 无错误 |
| **解码器 Drop** | 正确 | ❌ 主线程退出后 | ✅ 主线程退出前 |
| **清理完成** | 5s | 后台 2-3s | 后台 2-3s |

**关键设计决策**：
| 需求 | 方案 | 权衡 |
|------|-----|------|
| **CUDA 资源正确释放** | 短暂阻塞等待解码器 drop (300ms) | 牺牲 300ms 响应时间 |
| **UI 快速响应** | 超时后 detach 剩余清理 | 网络/进程清理异步 |
| **用户体验** | 90% 情况 <400ms 关闭窗口 | 可接受延迟 |

**经验总结**：
1. **永不使用 `thread.is_finished()` in stable Rust**：它是 nightly-only API
2. **超时 join 用 `std::mem::forget`**：比 detached spawn 更安全
3. **CUDA 资源必须在主线程存在时释放**：driver 退出后 context 失效
4. **退出顺序必须：解码器 → channel → socket → 进程**
5. **Hybrid cleanup 最优**：关键资源阻塞释放 + 次要资源 detach
6. **UI 响应优先，但硬件资源不可妥协**（CUDA > 用户体验极致化）

---

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

## 输入映射（新增）

### 13. 键盘映射坐标系与 capture-orientation 锁定问题 (2025-12-16) ✅ 已解决

**问题现象**：
- 使用 NVDEC 时，`capture-orientation=@0` 锁定视频方向为设备自然方向（0°竖屏）
- 用户将设备旋转到横屏（rotation=1, 90° CCW），触发 Profile 切换到 `rotation=1` 的配置
- 但键盘映射按键位置完全错误，点击目标不在预期位置

**根本原因**：
**Profile 坐标系与视频坐标系不一致**

1. **Profile.rotation**：记录配置对应的设备旋转角度（CCW，逆时针）
   - `rotation=0`: 0° 竖屏
   - `rotation=1`: 90° CCW 横屏（设备向左转）
   - `rotation=2`: 180°
   - `rotation=3`: 270° CCW 横屏（设备向右转）

2. **Profile 坐标系**：百分比坐标（0.0-1.0）基于 `profile.rotation` 对应的设备方向
   - 例如 `rotation=1` 的配置，坐标是基于"横屏设备"的坐标系

3. **视频坐标系（未锁定 capture-orientation）**：
   - 视频方向跟随设备旋转
   - `profile.rotation == device_orientation` 时 Profile 才匹配
   - 此时坐标系一致，直接缩放百分比即可

4. **视频坐标系（锁定 capture-orientation=@0）**：
   - **视频方向始终为 0°（设备自然方向/竖屏）**
   - 设备旋转到 `rotation=1` 时，Profile 被激活
   - 但视频坐标系仍是 0°，而 Profile 坐标是 90° CCW 坐标系
   - **坐标系不匹配 → 映射位置错误**

**错误代码**：
```rust
// ❌ 直接缩放，未考虑旋转差异
let (px, py) = (x_percent * video_width as f32, y_percent * video_height as f32);
```

**正确解决方案**：
```rust
/// 转换坐标时考虑 profile.rotation 与视频坐标系（0°）的差异
let transform_coord = |x_percent: f32, y_percent: f32| -> (f32, f32) {
    if !capture_orientation_locked {
        // 未锁定：视频坐标系跟随设备，直接缩放
        (x_percent * video_width as f32, y_percent * video_height as f32)
    } else {
        // 锁定：视频坐标系固定为 0°，需要旋转变换
        match profile.rotation {
            0 => (x_percent * video_width as f32, y_percent * video_height as f32),
            1 => {
                // Profile 坐标是 90° CCW 坐标系
                // 转换到 0°：(x', y') -> (y', 1-x')
                (y_percent * video_width as f32, (1.0 - x_percent) * video_height as f32)
            }
            2 => {
                // Profile 坐标是 180° 坐标系
                // 转换到 0°：(x', y') -> (1-x', 1-y')
                ((1.0 - x_percent) * video_width as f32, (1.0 - y_percent) * video_height as f32)
            }
            3 => {
                // Profile 坐标是 270° CCW 坐标系
                // 转换到 0°：(x', y') -> (1-y', x')
                ((1.0 - y_percent) * video_width as f32, x_percent * video_height as f32)
            }
            _ => (x_percent * video_width as f32, y_percent * video_height as f32),
        }
    }
};
```

**关键理解**：
1. **Android Display Rotation（device_orientation）**：逆时针（CCW）
   - 参考：`android.view.Surface.ROTATION_*`
2. **scrcpy capture-orientation**：顺时针（CW）表示，但内部转换为 CCW
   - 参考：`Orientation.java` line 37: `int cwRotation = (4 - ccwRotation) % 4`
3. **Profile.rotation 跟随 Android Display Rotation**（CCW）
4. **capture-orientation=@0 锁定后，视频固定为设备自然方向（0°）**

**实现修改**：
1. 在 `SAideApp` 中添加 `capture_orientation_locked: bool` 字段
2. 在 `InitEvent::ConnectionReady` 中传递该标志
3. 在 `KeyboardMapper::refresh_profiles()` 中接收参数
4. 在 `KeyboardMapper::update_pixel_mappings()` 中应用旋转变换

**测试验证**：
- 设备竖屏（rotation=0），Profile rotation=0：✅ 坐标正确
- 设备横屏（rotation=1），Profile rotation=1，capture locked：✅ 坐标自动转换正确
- 设备横屏（rotation=1），Profile rotation=1，capture unlocked：✅ 直接缩放正确
- 其他旋转角度（2, 3）：✅ 变换公式对称

**关键教训**：
1. **理解坐标系变换**：多个坐标系（Profile/视频/设备）需明确基准方向
2. **注意旋转方向定义**：CCW vs CW，不同系统定义不同
3. **锁定方向的副作用**：`capture-orientation` 锁定会导致坐标系不跟随设备旋转
4. **Profile.rotation 语义**：记录的是"配置对应的设备方向"，不是"视频方向"

**代码位置**：
- `src/controller/keyboard.rs` - 坐标转换逻辑（update_pixel_mappings）
- `src/app/init.rs` - capture_orientation_locked 标志传递
- `src/app/ui/saide.rs` - 状态存储和 refresh_profiles 调用
- `3rd-party/scrcpy/server/.../Orientation.java` - 旋转方向定义参考

---

**最后更新**: 2025-12-16
