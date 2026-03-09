# Code Quality Fixes

本文档记录经过深度分析后确认需要修复的问题，以及各修复方案的优劣对比与最终选择。

> **原则**：本项目以低延迟为最高优先级。丢帧、音频抖动是可接受的，不应因此增加复杂度。

---

## 已验证为合理实现（不需修复）

以下问题经与 `3rd-party/scrcpy/server` 源码对比后，确认为符合协议或低延迟设计选择：

| 问题 | 结论 | 原因 |
|------|------|------|
| `protocol/video.rs` 包头 `try_into().unwrap()` | ✅ 安全 | 切片长度固定为 8/4 字节，unwrap 不可能失败 |
| `packet.rs` 强制 `pts = dts` | ✅ 符合协议 | scrcpy-server 只发送音频/H.264，无 B 帧，PTS=DTS |
| `decoder/audio/opus.rs` 固定 960 样本缓冲区 | ✅ 符合协议 | scrcpy-server MediaCodec Opus 固定 20ms/960 samples@48kHz |
| `audio/player.rs` rtrb 满时静默丢弃 | ✅ 低延迟设计 | 已有 `debug!` 日志；低延迟场景下丢弃优于阻塞 |
| `audio/player.rs` Drop 中 sleep 50ms | ✅ 可接受 | cpal stream Drop 后 callback 终止，sleep 为防御性等待（见 Issue 4 注释改进） |
| `h264_parser.rs` `BitReader` 越界 | ✅ 安全 | `read_bit()` 越界返回 `None`，通过 `?` 传播，整个解析链安全 |

---

## 确认需修复的问题

### Issue 1：`h264_parser::read_ue()` 无上限循环

**文件**：`src/decoder/h264_parser.rs`

**问题描述**：
```rust
// 当前实现
fn read_ue(&mut self) -> Option<u32> {
    let mut leading_zeros = 0u32;
    while self.read_bit()? == 0 {   // 无上限 —— 恶意/损坏数据可造成长时间占用
        leading_zeros += 1;
    }
    // ...
}
```
指数哥伦布编码中，前导零位计数决定值的大小。合法 H.264 SPS 中 `read_ue()` 的值域有限（最大 32），但损坏或恶意数据可以提供大量连续零位，导致 CPU 占用直到数据耗尽。

**影响**：中度。正常 scrcpy 数据流不会触发；连接不可信设备或网络损坏时有风险。

**方案对比**：

| 方案 | 实现复杂度 | 性能影响 | 正确性 |
|------|----------|---------|--------|
| A. 加 `MAX_LEADING_ZEROS = 32` 上限，超出返回 `None` | 极低（1行） | 零 | ✅ H.264 spec 合规 |
| B. 使用 `nom` 等解析库 | 高（引入新依赖） | 微小 | ✅ |
| C. 保持现状，只加注释 | 零 | 零 | ❌ 风险未消除 |

**选择方案 A**：一行代码，零依赖，完全消除风险，符合 H.264 规范（Exp-Golomb 编码的实际值域上限远小于 32 前导零）。

---

### Issue 2：`i18n::current_bundle()` 在 bundles 为空时 panic

**文件**：`src/i18n/manager.rs`

**问题描述**：
```rust
fn current_bundle(&self) -> &FluentBundle<FluentResource> {
    self.bundles
        .get(&self.current_locale)
        .or_else(|| self.bundles.values().next())
        .expect("At least one bundle should exist")  // bundles 为空时 panic
}
```

当所有 `.ftl` 文件均加载失败（I/O 错误、文件缺失）时，`bundles` 为空，`expect` 触发 panic。

**影响**：低概率，但可将程序崩溃（而非静默降级）。

**方案对比**：

| 方案 | 实现复杂度 | 用户体验 | 调试性 |
|------|----------|---------|--------|
| A. `current_bundle()` 改为返回 `Option`，调用方 `get_with_fluent_args` 在 `None` 时直接返回 key 字符串 | 低（修改签名 + 调用方适配） | ✅ 降级显示 key | ✅ |
| B. 保留 `expect`，但在 `load()` 时强制插入一个空 bundle 确保非空 | 低 | ⚠️ 返回空字符串 | ❌ |
| C. 在 `new()` / `reload()` 时若无 bundle 则 `error!` 日志并插入 hardcoded fallback | 中 | ✅ | ✅ |

**选择方案 A**：最直接，不掩盖问题，降级为显示翻译 key 对开发者也有提示价值。`get_with_fluent_args` 已有 `unwrap_or_else(|| key.to_string())` fallback，只需把 `current_bundle()` 传播 `Option` 即可。

---

### Issue 3：`StreamPlayer::stop()` 未发送取消信号，未 join 工作线程

**文件**：`src/core/ui/player.rs`

**问题描述**：
```rust
pub fn stop(&mut self) {
    self.state = StreamPlayerState::Idle;
    self.frame_rx = None;       // drop 接收端，工作线程 send 会返回 Err
    self.stats_rx = None;
    self.current_frame = None;
    self.stream_thread.take(); // JoinHandle 被丢弃，线程异步运行直到自然退出
    // cancel_token.cancel() 未被调用
    // 没有 join，资源释放不确定
}
```

工作线程的退出依赖自身检测 channel send 失败，但：
1. `cancel_token` 已传入工作线程，`stop()` 未调用 `cancel_token.cancel()`，tokio 取消机制未被利用。
2. `JoinHandle` 被丢弃而非 join，线程异步存活，资源（FFmpeg 上下文、GPU 纹理）释放时机不确定。

**影响**：中度。低延迟场景下工作线程会很快自然退出，但快速 stop/start 循环可能导致资源竞争。

**方案对比**：

| 方案 | 实现复杂度 | 确定性 | 延迟影响 |
|------|----------|--------|---------|
| A. `stop()` 中调用 `cancel_token.cancel()`，然后 `join` handle（同步等待） | 低 | ✅ 完全确定 | 增加少量等待（线程需退出） |
| B. `stop()` 中调用 `cancel_token.cancel()`，不 join（仍然异步） | 极低 | ⚠️ 部分改进 | 零 |
| C. 保持现状，仅加注释 | 零 | ❌ | 零 |

**选择方案 A**：tokio `CancellationToken` 是专为此设计，调用 `cancel()` 触发工作线程内的 `token.cancelled().await` 快速退出，然后在调用线程侧 block_on join。stop 操作本身不在热路径上，少量等待可接受。

---

## Issue 4：AudioPlayer Drop 注释改进（无代码修改）

**文件**：`src/decoder/audio/player.rs`

现有 `sleep(50ms)` 是正确的：cpal `Stream` 析构后 callback 不再被调用，sleep 给当前正在执行的 callback 留出完成时间。但注释不够清晰。

**修改**：仅改进注释，无代码改动。

---

---

## Issue 5：`stop()` 未 drain `event_rx` 导致旧会话事件污染新状态

**文件**：`src/core/ui/player.rs`

**问题描述**：

`stop()` join 工作线程后，`event_rx` channel 中可能仍残留旧会话在退出前发出的 `PlayerEvent::Ready` 或 `PlayerEvent::Failed` 事件。这些事件会在 stop() 返回后被 `update()` 消费：

- `PlayerEvent::Ready`：将 `state` 从 `Idle` 改写回 `Streaming`，但线程已退出，所有接收端已失效。
- `PlayerEvent::Failed`：在已 Idle 的状态上叠加 `PlayerState::Failed`，产生错误提示。
- 快速 stop→start 场景：旧会话事件在新会话启动后被消费，污染新会话的状态机。

**影响**：中度。正常单次停止后 UI 会显示"No Device"与实际一致；但快速重连时（设备断线→自动重连）有概率出现短暂状态错乱。

**方案对比**：

| 方案 | 实现 | 正确性 |
|------|------|--------|
| A. stop() join 后立即 drain channel | `while self.event_rx.try_recv().is_ok() {}` | ✅ 完全消除旧事件 |
| B. update() 中检查 session_id 过滤旧事件 | 为每个 PlayerEvent 附加 session_id，update() 比对 | ✅ 精确，但实现复杂 |
| C. stop() 后重建 (event_tx, event_rx) 对 | `let (tx, rx) = bounded(...); self.event_tx = tx; self.event_rx = rx;` | ✅ 原子清空，但 start() 传给 worker 的 event_tx 已是旧引用的克隆，需在 start() 中传新 tx |

**选择方案 A**：drain 代价极低（最多消费 PLAYER_EVENT_BUFFER_SIZE=5 个 event），实现最简单，join 后线程已退出（不会再有新事件入队），drain 100% 安全。

**修复**（已实施）：

```rust
// 4. Drain stale events left in channel from the just-joined session.
let drained = {
    let mut count = 0u32;
    while self.event_rx.try_recv().is_ok() { count += 1; }
    count
};
if drained > 0 {
    debug!("Drained {drained} stale event(s) from previous session");
}
```

---

## 实现顺序

1. [x] 本文档
2. [x] Issue 1：`h264_parser.rs` `read_ue()` 加上限（1行改动）
3. [x] Issue 2：`i18n/manager.rs` `current_bundle()` 返回 `Option`
4. [x] Issue 3：`core/ui/player.rs` `stop()` 调用 cancel + join
5. [x] Issue 4：`decoder/audio/player.rs` Drop 注释改进
6. [x] Issue 5：`core/ui/player.rs` `stop()` drain event_rx
