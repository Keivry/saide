# SAide 开发陷阱记录

> **目的**: 记录开发过程中遇到的坑、反模式、失败案例，确保不再重复犯错

---

## 架构设计陷阱

### ❌ God Object 反模式

**位置**: `src/saide/ui/saide.rs:58-138`  
**问题**: `SAideApp` 包含 40+ 字段，违反单一职责原则

```rust
// ❌ 错误示例：所有状态混在一起
pub struct SAideApp {
    shutdown_rx: Receiver<()>,
    toolbar: Toolbar,
    player: StreamPlayer,
    connection: Option<ScrcpyConnection>,
    keyboard_mapper: Option<KeyboardMapper>,
    // ... 35 more fields
}
```

**教训**:
1. 单个结构体字段超过 15 个时应考虑拆分
2. 按职责分组：UI 状态、业务逻辑、配置应分离
3. 使用组合而非继承/聚合所有功能

**正确做法**:
```rust
// ✅ 正确示例：按职责分组
pub struct SAideApp {
    ui_state: UIState,          // 工具栏、指示器、播放器
    app_state: AppState,        // 连接、映射器
    config: Arc<ConfigManager>,
}
```

---

### ❌ 错误模块循环依赖

**位置**: `src/error.rs:11-15`  
**问题**: 通用错误模块依赖特定业务模块（decoder）

```rust
// ❌ 错误示例：error 依赖 decoder
use super::decoder::{VideoError, audio::AudioError};

pub enum SAideError {
    VideoError(VideoError),  // 循环依赖！
    AudioError(AudioError),
}
```

**教训**:
1. 错误类型应与业务模块放在一起（`decoder/error.rs`）
2. 顶层错误模块仅定义聚合类型和转换规则
3. 遵循依赖倒置原则：高层模块不应依赖低层模块

**正确做法**:
```rust
// ✅ decoder/error.rs
pub enum VideoError { ... }

// ✅ error.rs
pub enum SAideError {
    Video(String),  // 或使用 Box<dyn Error>
    Audio(String),
}
```

---

## FFmpeg 集成陷阱

### ❌ Unsafe 回调无边界检查

**位置**: `src/decoder/nvdec.rs:301-320`  
**问题**: FFmpeg 回调中裸指针操作无边界检查

```rust
// ❌ 错误示例：无边界检查的裸指针访问
unsafe extern "C" fn get_cuda_format(...) -> i32 {
    let fmts = hw_pix_fmts as *const i32;
    for n in 0.. {  // 无上界！
        if *fmts.add(n) == -1 { break; }
    }
}
```

**教训**:
1. FFmpeg 回调在独立线程执行，panic 会导致进程崩溃
2. 裸指针操作必须添加合理性检查（最大迭代次数）
3. 使用 `slice::from_raw_parts` 替代手动指针算术

**正确做法**:
```rust
// ✅ 正确示例：添加边界和安全转换
unsafe extern "C" fn get_cuda_format(...) -> i32 {
    const MAX_FORMATS: usize = 8;
    let slice = std::slice::from_raw_parts(hw_pix_fmts as *const i32, MAX_FORMATS);
    for &fmt in slice {
        if fmt == -1 { break; }
        // ...
    }
}
```

---

### ❌ 忽略解码器时序假设

**位置**: `src/saide/ui/player.rs:453-728`  
**问题**: 假设视频/音频帧按严格顺序到达

**教训**:
1. Scrcpy 协议不保证音视频帧严格交错
2. 网络抖动可能导致帧乱序或丢失
3. 必须实现独立的音视频队列 + 时间戳同步

**正确做法**:
- 使用 PTS（presentation timestamp）重排序
- 音频使用独立线程 + 环形缓冲
- 视频使用帧队列 + 丢帧策略

---

## Rust 语言陷阱

### ❌ unwrap() 滥用

**位置**: `src/saide/ui/saide.rs:454,622,685,...`（68 处）  
**问题**: 在生产代码路径使用 `unwrap()` 导致潜在 panic

```rust
// ❌ 错误示例：Option unwrap 无错误上下文
self.keyboard_mapper.unwrap().process_event();
```

**教训**:
1. 仅在测试代码或确定不会失败的场景使用 `unwrap()`
2. 生产代码使用 `expect()` 提供错误上下文
3. 优先使用 `if let` 或 `?` 操作符

**正确做法**:
```rust
// ✅ 正确示例：使用 if let 避免 panic
if let Some(mapper) = self.keyboard_mapper.as_mut() {
    mapper.process_event();
}

// ✅ 或使用 expect 提供上下文
self.keyboard_mapper
    .as_mut()
    .expect("keyboard_mapper should be initialized after connection")
    .process_event();
```

---

### ❌ unreachable!() 误用

**位置**: `src/saide/coords.rs:152,194,275,326`  
**问题**: 在取模运算后使用 `unreachable!()`

```rust
// ❌ 错误示例：整数溢出可能导致 unreachable 被触发
let rotation = (self.device_orientation + capture_orient) % 4;
match rotation {
    0 | 1 | 2 | 3 => { /* ... */ }
    _ => unreachable!(),  // 如果溢出，仍会触发！
}
```

**教训**:
1. `unreachable!()` 仅用于逻辑上不可能的分支
2. 涉及外部输入或计算的值应使用 `Result` 或 `debug_assert!`
3. 优先使用穷举匹配（无 `_` 分支）

**正确做法**:
```rust
// ✅ 正确示例：穷举匹配或返回 Result
let rotation = (self.device_orientation + capture_orient) % 4;
match rotation {
    0 => { /* ... */ }
    1 => { /* ... */ }
    2 => { /* ... */ }
    3 => { /* ... */ }
    _ => {
        debug_assert!(false, "rotation should be 0-3, got {}", rotation);
        // 降级处理：使用默认值
        return default_position();
    }
}
```

---

## ADB 集成陷阱

### ❌ 假设 ADB 在 PATH 中

**位置**: `src/controller/adb.rs`, `src/scrcpy/server.rs`  
**问题**: 多处 `Command::new("adb")` 未验证可执行性

**教训**:
1. 不同系统 ADB 路径不同（Linux: `/usr/bin/adb`, Windows: `platform-tools/adb.exe`）
2. 用户可能未安装 Android SDK
3. 应在启动时验证并缓存 ADB 路径

**正确做法**:
```rust
// ✅ 启动时验证
impl AdbShell {
    pub fn verify_adb() -> Result<PathBuf> {
        let adb_path = which::which("adb")
            .or_else(|_| std::env::var("ADB_PATH").map(PathBuf::from))
            .map_err(|_| SAideError::AdbError("ADB not found in PATH".into()))?;
        
        // 测试执行
        Command::new(&adb_path).arg("version").status()?;
        Ok(adb_path)
    }
}
```

---

### ❌ 忽略 ADB 输出格式差异

**位置**: `src/controller/adb.rs:89-95`  
**问题**: 仅解析新版 Android 输出格式

```rust
// ❌ 错误示例：仅识别 "SurfaceOrientation: 1" 格式
let output = String::from_utf8_lossy(&result.stdout);
if let Some(line) = output.lines().find(|l| l.contains("SurfaceOrientation")) {
    // 旧版 Android 输出 "mCurrentRotation=1" 会解析失败
}
```

**教训**:
1. Android 不同版本输出格式不同
2. 应支持多种正则模式或 fallback 机制
3. 解析失败应记录日志而非静默忽略

**正确做法**:
```rust
// ✅ 支持多种格式
fn parse_orientation(output: &str) -> Option<u32> {
    // 新版: "SurfaceOrientation: 1"
    if let Some(captures) = Regex::new(r"SurfaceOrientation:\s*(\d)")
        .unwrap()
        .captures(output) {
        return captures.get(1)?.as_str().parse().ok();
    }
    
    // 旧版: "mCurrentRotation=1"
    if let Some(captures) = Regex::new(r"mCurrentRotation=(\d)")
        .unwrap()
        .captures(output) {
        return captures.get(1)?.as_str().parse().ok();
    }
    
    warn!("Unknown orientation format: {}", output);
    None
}
```

---

## 配置管理陷阱

### ❌ 非原子文件写入

**位置**: `src/config/mod.rs`  
**问题**: 直接覆盖配置文件，崩溃时可能损坏

**教训**:
1. 直接 `fs::write()` 在写入中途崩溃会导致文件损坏
2. 必须使用临时文件 + `rename()` 保证原子性
3. Windows 需额外处理文件锁

**正确做法**:
```rust
// ✅ 原子写入
pub fn save_atomic(&self, path: &Path, content: &str) -> Result<()> {
    let tmp_path = path.with_extension("tmp");
    fs::write(&tmp_path, content)?;
    fs::rename(&tmp_path, path)?;  // POSIX 保证原子性
    Ok(())
}
```

---

### ❌ 硬编码配置散落各处

**位置**: `src/main.rs:18-19`, `src/scrcpy/server.rs:103`, 等 15+ 处  
**问题**: 配置值分散在多个文件，难以统一修改

**教训**:
1. 所有可调整的值应集中在 `config.toml` 或 `constant.rs`
2. 魔法数字应定义为命名常量
3. UI 相关配置（窗口尺寸、字体大小）应支持运行时调整

**正确做法**:
```rust
// ✅ config.toml
[window]
default_width = 1280
default_height = 720
min_width = 640
min_height = 480

[video]
max_size = 1600
max_fps = 60
```

---

## 多线程陷阱

### ❌ 混用同步原语

**位置**: `src/saide/ui/player.rs` (parking_lot), `src/decoder/audio/player.rs` (RefCell)  
**问题**: 无统一同步策略，容易混淆

**教训**:
1. 单线程场景用 `RefCell`
2. 多线程需可变性用 `Mutex`（优先 `parking_lot::Mutex`，更快）
3. 多线程只读用 `Arc`
4. 制定统一规范并在代码审查中检查

**规范**:
| 场景 | 原语 | 示例 |
|------|------|------|
| UI 单线程可变 | `RefCell` | egui 回调中修改状态 |
| 跨线程共享可变 | `parking_lot::Mutex` | 解码器线程更新帧缓冲 |
| 跨线程共享只读 | `Arc` | 配置对象 |
| 原子计数器 | `AtomicU64` | FPS 计数、丢帧统计 |

---

### ❌ 音频回调中 panic

**位置**: `src/decoder/audio/player.rs:94-96`  
**问题**: cpal 回调在独立线程，panic 导致音频静音且难以调试

**教训**:
1. 音频回调必须保证 panic-free
2. 使用 `if let Ok(...) = ...` 模式处理所有 Result
3. 错误计数器记录问题而非 panic

**正确做法**:
```rust
// ✅ Panic-free 音频回调
fn audio_callback(data: &mut [f32], consumer: &mut Consumer<f32>, underruns: &AtomicU64) {
    match consumer.read_chunk(data.len()) {
        Ok(chunk) => {
            data.copy_from_slice(chunk.as_slice());
            chunk.commit_all();
        }
        Err(_) => {
            // 欠载：填充静音
            data.fill(0.0);
            underruns.fetch_add(1, Ordering::Relaxed);
        }
    }
}
```

---

## egui UI 陷阱

### ❌ 每帧重建大型结构

**位置**: `src/saide/ui/saide.rs`  
**问题**: 在 `update()` 中重复分配内存

**教训**:
1. egui 的 `update()` 每秒调用 60 次
2. 避免在其中创建 `Vec`, `String` 等堆分配
3. 使用 `retain` 或 `SmallVec` 优化

**正确做法**:
```rust
// ❌ 每帧分配
fn update(&mut self, ctx: &egui::Context) {
    let items = vec![item1, item2, item3];  // 每帧 60 次分配
    for item in items { /* ... */ }
}

// ✅ 复用缓冲
struct UIState {
    items_buffer: Vec<Item>,
}

fn update(&mut self, ctx: &egui::Context) {
    self.items_buffer.clear();
    self.items_buffer.extend([item1, item2, item3]);
    for item in &self.items_buffer { /* ... */ }
}
```

---

## 总结

### 架构原则
1. 单一职责：结构体字段 <15 个，函数 <100 行
2. 依赖倒置：错误类型跟业务模块走，顶层仅聚合
3. 明确所有权：优先显式 `shutdown()`，`Drop` 仅兜底

### Rust 最佳实践
1. 生产代码禁用 `unwrap()`，用 `expect()` 或 `if let`
2. `unreachable!()` 仅用于编译时可证明的分支
3. 多线程统一使用 `parking_lot::Mutex`

### 外部集成
1. FFmpeg 回调必须 panic-free + 边界检查
2. ADB 命令支持多版本输出格式
3. 配置文件使用原子写入

### 性能优化
1. egui UI 循环避免堆分配
2. 音频回调使用无锁结构（`rtrb`）
3. 视频解码优先硬件加速（VAAPI/NVDEC）

---

## 音频延迟优化陷阱 (Phase 3)

### ❌ cpal 独占模式限制

**调查日期**: 2026-01-15  
**cpal 版本**: 0.17.0-0.17.1  
**位置**: `src/decoder/audio/player.rs`

**问题**: cpal **不支持**独占模式 API（WASAPI exclusive / ALSA exclusive）

**证据**:
```rust
// ❌ 错误期望：cpal 应该有这些 API（实际没有）
let stream = device.build_output_stream(
    &config,
    data_callback,
    error_callback,
    timeout,
    // ❌ 不存在的参数：
    // exclusive_mode: true,  // cpal 0.17 无此参数
    // stream_priority: High, // cpal 0.17 无此参数
)?;
```

**调查结果**:
- 检查 `build_output_stream()` 签名：
  - 参数仅有：`config`, `data_callback`, `error_callback`, `timeout`
  - 无独占模式标志或流优先级选项
- 搜索 cpal 源码：
  - 仅在内部 mutex 注释中提到 "exclusive"
  - 未暴露平台特定的独占访问 API（WASAPI/ALSA）

**教训**:
1. ❌ **不要假设音频库支持低级平台特性** - 必须先查文档确认 API
2. ⚠️ 独占模式需绕过 OS 音频混音器，cpal 走的是高层抽象（跨平台一致性优先）
3. ✅ 替代方案：**缓冲区大小优化**（已实现：128→64 frames）

**替代优化** (已实施):
- 减少 `AUDIO_BUFFER_FRAMES` 从 128→64（1.33ms 延迟降低）
- 可通过 `[scrcpy.audio] buffer_frames` 在 config.toml 配置

**未来考虑**:
- 监控 cpal v0.18+ 是否添加独占模式 API
- 如需极低延迟，考虑平台特定库（wasapi-rs, alsa）
- 当前 64 frames 已接近硬件限制，进一步优化收益递减

---

### 音频缓冲调优指南

**默认**: 64 frames (1.33ms @ 48kHz) - 低延迟优化

#### 缓冲过小症状（欠载/underrun）

**表现**:
- 音频爆音、卡顿
- 频繁静音间隙
- 日志显示高 underrun 计数：`AudioPlayer stopped (N underruns)`

**根本原因**:
```
音频回调周期 < 数据生成周期
├─ 缓冲 64 frames = 每 1.33ms 请求新数据
├─ 如果解码/网络延迟 > 1.33ms
└─ 回调读空缓冲 → 填充静音 → 爆音
```

#### 解决方案（按优先级）

**1. 增加缓冲区大小** (配置文件调优)

```toml
# ~/.config/saide/config.toml
[scrcpy.audio]
buffer_frames = 128  # 2.67ms (更稳定)
# 或
buffer_frames = 256  # 5.33ms (非常稳定，延迟稍高)
```

**2. 系统特定推荐值**

| 平台 | 推荐值 (frames) | 延迟 @ 48kHz | 说明 |
|------|----------------|-------------|------|
| 树莓派 / 低功耗 | 256-512 | 5.33-10.67ms | CPU 弱，解码慢 |
| 桌面 Linux (PulseAudio) | 128-256 | 2.67-5.33ms | 混音器开销 |
| 桌面 Linux (PipeWire) | 64-128 | 1.33-2.67ms | 低延迟混音器 |
| Windows (WASAPI shared) | 128-256 | 2.67-5.33ms | 共享模式开销 |
| macOS (CoreAudio) | 64-128 | 1.33-2.67ms | 硬件性能优秀 |

**3. 监控 underrun 率**

```bash
# 查看日志中的 underrun 计数
journalctl -f | grep "Audio player"

# 正常范围：
# "Audio player stopped (15 underruns)"  ✅ 可接受（0.1% @ 60s）
# "Audio player stopped (500 underruns)" ❌ 缓冲过小（5% @ 60s）
```

**4. 延迟 vs 稳定性权衡**

```
64 frames  = 1.33ms 延迟  (激进，可能欠载)  ⚡ Phase 3 默认
128 frames = 2.67ms 延迟  (平衡，推荐)     ✅ Phase 1 默认
256 frames = 5.33ms 延迟  (安全，弱系统)   🛡️ 保守选择
512 frames = 10.67ms 延迟 (极稳，高延迟)   🐌 特殊场景
```

#### 调试流程

```bash
# 1. 运行应用，播放音频
./saide

# 2. 观察日志
# 预期：
# "Audio player started: 48000Hz, 2 channels, buffer=64frames (1.33ms)"
# "Audio player stopped (0-50 underruns)"  # 正常范围

# 3. 如果 underrun > 100:
#    编辑 ~/.config/saide/config.toml
#    设置 buffer_frames = 128
#    重启应用测试

# 4. 如果 underrun > 200:
#    设置 buffer_frames = 256
#    重启应用测试

# 5. 如果仍有问题:
#    检查系统负载 (top/htop)
#    检查网络延迟 (ping 设备)
#    考虑硬件限制或设备端问题
```

#### 技术原理

**缓冲区作用**:
```
解码线程 → [Ring Buffer] → 音频回调线程
            ↑               ↓
            写入            读取 (每 1.33ms @ 64 frames)
            
缓冲越小：
✅ 延迟越低（数据新鲜度高）
❌ 抗抖动能力弱（网络波动敏感）

缓冲越大：
✅ 稳定性强（容忍网络抖动）
❌ 延迟越高（数据老旧）
```

**Phase 3 设计哲学**:
- 默认 64 frames：**激进低延迟**（假设用户有良好网络 + 中高端硬件）
- 配置可调：**用户自主权**（弱系统可自行提高缓冲）
- 监控 underrun：**可观测性**（日志显示性能是否满足需求）

**历史变更**:
```
Phase 1: 128 frames (2.67ms) - 稳定优先
Phase 3: 64 frames (1.33ms)  - 低延迟优先，可配置回退
```
