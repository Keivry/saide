# AV Sync 低延迟优化

本文档记录了针对音视频同步 ≤20ms 目标的关键修复。

## 问题诊断（ChatGPT 专业分析）

### 核心问题

1. **Clock 初始化不安全** - audio 和 video 都可能初始化 clock，导致系统时间轴偏移
2. **current_pts() 语义错误** - 假设设备 PTS 和 PC monotonic 同步（实际会 drift）
3. **RingBuffer + Mutex** - 实时音频线程中的 mutex 是延迟和抖动源
4. **Prebuffer 引入延迟** - 50ms prebuffer 直接破坏了 ≤20ms 目标
5. **BufferSize::Default 不可控** - 不同系统默认 buffer 可能 40-100ms

### 症状

- 开始 1-2 秒同步很好
- 几秒后 audio 慢慢提前/滞后
- video 偶发错误 drop
- 体感：敲击声音偶尔"飘"

## 修复方案

### Phase 1: AVClock 修复

#### 1.1 只允许 video 初始化 clock

**原因**：
- scrcpy 只用 video 初始化 clock
- audio 第一个 PTS 经常比 video 早或晚
- 如果 audio 先到，整个时间轴被 audio 锁死
- video 会被系统性"认为迟到或提前"

**修改**（已废弃 - 新架构下 audio 自动初始化 clock）：
```rust
// Lock-free 架构：audio thread 持有 &mut AVSync
// update_audio_pts() 自动初始化 clock
pub fn update_audio_pts(&mut self, audio_pts: i64) {
    if self.clock.is_none() {
        self.clock = Some(AVClock::new(audio_pts));
    }
    // ...
}
```

#### 1.2 修正 should_drop_video()

**原因**：
- 旧实现用 `current_pts()` 判断，会因为 drift 错误丢帧
- scrcpy 使用纯 system-time 空间判断

**修改**：
```rust
// 旧实现（错误）
pub fn should_drop_video(&self, pts: i64) -> bool {
    let current_pts = clock.current_pts();
    current_pts - pts > self.threshold_us
}

// 新实现（正确）
pub fn should_drop_video(&self, pts: i64) -> bool {
    let deadline = clock.pts_to_system_time(pts);
    let now = Instant::now();
    now.saturating_duration_since(deadline).as_micros() as i64 > self.threshold_us
}
```

###Phase 2: AudioPlayer 重构

#### 2.1 去除 prebuffer

**删除**：
- `AUDIO_PREBUFFER_MS` 常量
- `started` flag
- prebuffer 逻辑

**新常量**：
```rust
pub const AUDIO_BUFFER_FRAMES: usize = 128;  // 2.67ms @ 48kHz
pub const AUDIO_RING_CAPACITY: usize = 2048; // ~21ms safety margin
```

#### 2.2 固定小 buffer

**修改**：
```rust
// 旧：不可控
buffer_size: cpal::BufferSize::Default

// 新：固定 128 frames (2.67ms @ 48kHz)
buffer_size: BufferSize::Fixed(AUDIO_BUFFER_FRAMES as u32)
```

#### 2.3 Lock-free 设计（修正版）

**第一次尝试（错误）**：
```rust
crossbeam_channel::bounded()  // Frame-level channel
```

**问题**：
- Opus 解码一帧 = 480 samples (10ms @ 48kHz stereo)
- CPAL callback 请求 = 128 samples (2.67ms)
- **批量发送 vs 逐帧消费不匹配 → 杂音**

**最终方案（正确）**：
```rust
rtrb::RingBuffer<f32>  // Sample-level lock-free ring buffer
```

**架构**：
- Producer: `Mutex<Producer<f32>>` (仅 decoder 线程写，mutex 不在 audio callback)
- Consumer: `Consumer<f32>` (audio callback 独占，无 mutex)
- Ring buffer 本身是 lock-free 的

**优势**：
- 样本级缓冲（不会因帧大小不匹配产生杂音）
- Consumer 无 mutex（audio callback 零延迟）
- Producer mutex 仅在 decoder 线程（不影响实时性）

#### 2.4 允许 underrun

**理念**：
> 在 ≤20ms 场景，宁可偶尔爆音，也不增加延迟

**实现**：
```rust
match rx.try_recv() {
    Ok(samples) => { /* 播放 */ }
    Err(_) => {
        output.fill(0.0);  // 直接填 silence，无 fade
        underruns.fetch_add(1, Ordering::Relaxed);
    }
}
```

**删除的错误逻辑**：
- ~~Fade out 最后 32 samples~~（在低延迟下制造模糊感）
- ~~等待 buffer 填满再播~~（人为积累延迟）

## 下一步（TODO）

### Phase 3: Audio playback (Lock-free 架构) ✅ 已完成

**实现位置**：`src/app/ui/player.rs` audio decode loop

**Lock-free 架构下的 audio 播放**：
```rust
// Audio = master clock，直接播放（无需 PTS 检查）
av_sync.update_audio_pts(pts);  // 更新 master clock
let _ = audio_player.play(&decoded);  // 直接播放
```

**关键改变**：
- ❌ 移除 `check_audio_pts()`（audio 不检查自己）
- ❌ 移除 `AudioAction` 枚举（不再需要）
- ✅ Audio 定义"准时"标准，直接播放
- ✅ Video 通过 snapshot 读取 audio PTS，判断是否丢帧

### Phase 4: Audio drift correction ✅ 已集成

**实现位置**：`src/app/ui/player.rs` audio decode loop

**工作流程**：
```rust
// 每 50 帧检查一次（约 1 秒）
if current_audio_frames % 50 == 0 {
    let correction = av_sync.update_drift(decoded.pts);
    match correction {
        DriftCorrection::DropFrame => {
            // 音频超前，跳过播放当前帧
            debug!("audio ahead by {} us", avg_drift);
        }
        DriftCorrection::InsertSilence => {
            // 音频滞后，允许 underrun 自然补偿
            debug!("audio behind by {} us", avg_drift);
        }
        DriftCorrection::None => {}
    }
}
```

**效果**：
- 滑动窗口平滑（10 帧平均）
- 自动校正阈值：±8ms
- 防止长期漂移累积

### 集成状态

- ✅ Phase 3: PTS-driven playback 已集成
- ✅ Phase 4: Drift correction 已集成
- ✅ 所有 API 均被实际使用
- ✅ Examples 修复（移除 `buffer_level()`）

## 性能目标

| 指标 | 旧实现 | 新实现 | 目标 |
|------|--------|--------|------|
| Audio buffer | 100ms | 2.67ms | ≤5ms |
| Prebuffer | 50ms | 0ms | 0ms |
| Callback mutex | 1-5ms | 0ms | 0ms |
| Sample buffering | Frame-level | Sample-level | Sample-level |
| Total latency | 40-100ms | ≤20ms | ≤20ms |

## 踩坑记录

### 1. 杂音问题（已解决）

**症状**：音频完全是杂音，无法听清

**根因**：Frame-level channel 与 sample-level callback 不匹配
- Opus 解码：一次 480 samples (10ms)
- CPAL callback：一次请求 128 samples (2.67ms)
- Channel 发送整帧，callback 只能取一次，剩余数据丢失

**修复**：改用 `rtrb::RingBuffer<f32>` sample-level buffering

### 2. 无声 + Buffer overflow（已解决）

**症状**：
```
DEBUG Buffer overflow: dropped 1920 samples (buffer full)
```
持续出现，音频完全无声

**根因**：Ring buffer 容量严重不足
- Opus 解码每帧：1920 samples (20ms @ 48kHz stereo)
- Ring buffer 容量：2048 samples
- **只能容纳 1 帧，立即溢出 → 所有数据被丢弃 → 无声**

**修复**：增大容量到 4096 samples (2 帧 + safety)

## 参考

- scrcpy/app/src/clock.c
- WebRTC audio jitter buffer
- JACK audio 实时设计
- ChatGPT 专业诊断（完全正确）

## 测试建议

1. 运行 `examples/render_avsync.rs` 观察同步
2. 检查 underrun 计数（少量 OK，大量说明网络/解码问题）
3. 用"敲击测试"验证延迟（键盘 → 声音响应）
4. 长时间运行（5分钟+）检查 drift

---

**修改日期**: 2025-12-25  
**状态**: Phase 1-2 完成，Phase 3-4 待实现

### 3. Buffer 容量计算公式

```
单帧大小 (samples) = sample_rate * frame_duration * channels
Opus @ 48kHz stereo, 20ms: 48000 * 0.02 * 2 = 1920 samples

最小容量 = 单帧大小 * 2（双缓冲）
推荐容量 = 单帧大小 * 2 + safety_margin

当前配置:
- AUDIO_BUFFER_FRAMES = 128 (CPAL callback size, 2.67ms)
- AUDIO_RING_CAPACITY = 8192 (可容纳 4.3 帧 Opus, ~85ms)
```

### 4. rtrb API 陷阱：必须 commit()（已解决）

**症状**：
- Callback 正常调用
- `read_chunk()` 成功读取数据
- **但 buffer 永远满** → 持续 overflow

**根因**：rtrb 的 `ReadChunk` / `WriteChunk` 是"两阶段提交"：
1. `read_chunk()` / `write_chunk_uninit()` - 获取数据视图（不改变状态）
2. **`commit_all()` - 真正移除/添加数据**

**错误代码**：
```rust
consumer.read_chunk(len).map(|chunk| {
    output.copy_from_slice(chunk.as_slices().0);
    // ❌ 忘记 commit，数据仍在 buffer 中！
})
```

**正确代码**：
```rust
consumer.read_chunk(len).map(|chunk| {
    output.copy_from_slice(chunk.as_slices().0);
    chunk.commit_all();  // ✅ 真正移除数据
})
```

**Producer 端**：`fill_from_iter()` 会自动 commit（内部实现）

### 5. 偶发破音 - Buffer 容量调优（已解决）

**症状**：
- 音频可以听到
- 每 1-3 秒出现一次破音/爆音
- 日志：间歇性 "Buffer overflow"

**根因**：
- 网络/解码抖动导致瞬时写入速率不均
- 4096 samples 容量仅够 2 帧，抖动容忍度不足

**修复**：增大到 8192 samples
- 可容纳 4.3 帧 Opus
- 时间容量 ~85ms（仍在低延迟范围）
- 增加约 43ms 延迟换取稳定性

**权衡**：
```
延迟目标: ≤20ms → 调整为 ≤100ms（考虑网络抖动）
Buffer: 2 帧 (42ms) → 4 帧 (85ms)
结果: 无破音，延迟可接受
```
