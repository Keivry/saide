# SAide 开发陷阱记录

> **目的**: 记录开发过程中遇到的坑、反模式、失败案例,确保不再重复犯错

---

## 并发安全陷阱 (P0 优先级)

> **2026-01-16 更新**: 修复音频播放器线程安全 bug

### ✅ FIXED: RefCell 在多线程环境误用 (commit PENDING)

**位置**: `src/decoder/audio/player.rs:39` (已修复)  
**问题**: `RefCell<Producer<f32>>` 在多线程音频回调中非线程安全

**修复前**:
```rust
// ❌ 错误示例: RefCell 不是 Sync, 跨线程访问会 panic
pub struct AudioPlayer {
    producer: RefCell<Producer<f32>>,  // 单线程借用检查
    // ...
}

impl AudioPlayer {
    pub fn play(&self, audio: &DecodedAudio) -> Result<()> {
        // 主线程调用
        self.producer.borrow_mut().write_chunk_uninit(...);
        // ...
    }
}

// 音频回调线程可能同时访问 producer → 运行时 panic!
```

**修复后**:
```rust
// ✅ 正确示例: 使用 Mutex 提供线程安全的内部可变性
pub struct AudioPlayer {
    producer: Arc<Mutex<Producer<f32>>>,  // 线程安全访问
    // ...
}

impl AudioPlayer {
    pub fn play(&self, audio: &DecodedAudio) -> Result<()> {
        // Mutex 保护并发访问
        self.producer
            .lock()
            .expect("Mutex poisoned")
            .write_chunk_uninit(...);
        // ...
    }
}
```

**教训**:
1. **RefCell 只用于单线程内部可变性**, 多线程必须用 `Mutex`/`RwLock`
2. 音频回调在独立线程执行 (`cpal::build_output_stream` 的 callback 闭包)
3. `rtrb::Producer` 本身是 lock-free, 但需要 `Mutex` 保护跨线程访问
4. 编译器无法检测这类错误 (通过 `Arc` 共享到线程绕过了 `!Sync` 检查)
5. **Pattern**: 所有可能跨线程共享的可变状态必须用 `Arc<Mutex<T>>`

**性能影响**:
- Mutex 开销: ~10-50ns (现代 CPU 上未竞争时)
- `rtrb::Producer` 内部已是 lock-free 环形缓冲区, Mutex 仅保护 producer 对象本身
- 实际测试: 音频延迟无明显变化 (1.3ms @ 64 frames, 48kHz)

---

## Panic 预防陷阱 (P0 优先级)

> **2026-01-15 更新**: P0.1-P0.9 已全部修复（9/9 完成）

### ✅ FIXED: Option::unwrap() 在初始化竞态 (commit 9e3b9a6)

**位置**: `src/saide/ui/saide.rs` 多处 (已修复)  
**问题**: `keyboard_mapper.as_ref().unwrap()` 在初始化未完成时 panic

**修复前**:
```rust
// ❌ 错误示例：InitEvent 未触发时 panic
fn process_keyboard_event(&self, key: &Key) {
    let keyboard_mapper = self.keyboard_mapper.as_ref().unwrap();  // panic!
    keyboard_mapper.handle_key_event(key);
}
```

**修复后**:
```rust
// ✅ 正确示例：优雅降级 + 日志
fn process_keyboard_event(&self, key: &Key) {
    let Some(keyboard_mapper) = &self.keyboard_mapper else {
        debug!("Keyboard mapper not available, ignoring key event");
        return Ok(false);
    };
    keyboard_mapper.handle_key_event(key)?;
}
```

**教训**:
1. **所有 Option 字段必须用 `if let Some(...)` 或 `let Some(...) else`**
2. 注释说"Safe unwrap"不能替代实际的错误处理
3. 初始化是异步的 (InitEvent)，事件处理可能先于初始化触发
4. 早期返回比 panic 更好 - 应用可以继续运行

**Pattern**: 优先使用 `let Some(x) = &self.field else { return; }` 而非 `if let Some(...)`

---

### ✅ FIXED: slice try_into().unwrap() (commit 673e8ee)

**位置**: `src/scrcpy/connection.rs:190-192` (已修复)  
**问题**: `codec_meta[0..4].try_into().unwrap()` 缺少错误上下文

**修复前**:
```rust
// ❌ 错误示例：panic 没有说明为什么安全
let codec_id = u32::from_be_bytes(codec_meta[0..4].try_into().unwrap());
```

**修复后**:
```rust
// ✅ 正确示例：用 expect() 标注不变式
let codec_id = u32::from_be_bytes(
    codec_meta[0..4].try_into()
        .expect("BUG: slice [0..4] from [u8; 12] must be 4 bytes")
);
```

**教训**:
1. `.unwrap()` 隐藏了代码意图，`.expect("...")` 显式说明为何安全
2. "BUG:" 前缀标识这是编程错误而非运行时错误
3. 描述不变式有助于未来维护者理解代码假设
4. 即使"永不 panic"的代码也应该文档化这个保证

**Pattern**: 所有 `.unwrap()` 必须改为 `.expect("BUG: <invariant>")`（测试代码除外）

---

### ✅ FIXED: unreachable!() 在取模后 (commit 7c6a4fa)

**位置**: `src/saide/coords.rs:152,194,275,326` (已修复)  
**问题**: 在取模运算后使用 `unreachable!()`，整数溢出可能触发

**修复前**:
```rust
// ❌ 错误示例：整数溢出可能导致 unreachable 被触发
let rotation = (self.device_orientation + capture_orient) % 4;
match rotation {
    0 | 1 | 2 | 3 => { /* ... */ }
    _ => unreachable!(),  // 如果溢出，仍会触发！
}
```

**修复后**:
```rust
// ✅ 正确示例：归一化 + debug_assert + 降级处理
self.orientation = value % 4;  // 归一化保证有效范围
match rotation {
    0 => { /* ... */ }
    1 => { /* ... */ }
    2 => { /* ... */ }
    3 => { /* ... */ }
    _ => {
        debug_assert!(false, "rotation should be 0-3 after % 4, got {}", rotation);
        default_position()  // 降级处理
    }
}
```

**教训**:
1. `unreachable!()` 仅用于逻辑上绝对不可能的分支
2. 涉及外部输入或计算的值应使用 `debug_assert!` + fallback
3. 优先归一化输入（`% 4`）而非信任计算结果
4. Release 模式下 `debug_assert!` 被移除，fallback 保证不 panic

**Pattern**: `debug_assert!(false); fallback_value` 优于 `unreachable!()`

---

### ✅ FIXED: cpal 音频回调边界检查 (commit 5f6b793)

**位置**: `src/decoder/audio/player.rs:95` (已修复)  
**问题**: cpal 音频回调中缺少数组边界检查，可能导致数组越界 panic

**修复前**:
```rust
// ❌ 错误示例：未检查 buffer 长度
data_callback: move |data: &mut [f32], _: &_| {
    for sample in data.iter_mut() {
        *sample = rx.recv().unwrap();  // 可能越界或 panic
    }
}
```

**修复后**:
```rust
// ✅ 正确示例：边界检查 + 优雅降级
data_callback: move |data: &mut [f32], _: &_| {
    for sample in data.iter_mut() {
        match rx.try_recv() {
            Ok(s) => *sample = s,
            Err(_) => *sample = 0.0,  // 静音填充
        }
    }
}
```

**教训**:
1. 音频回调在独立线程执行，panic 会导致进程崩溃
2. 必须使用 `try_recv()` 而非 `recv()` 避免阻塞
3. 无数据时用静音填充（`0.0`）而非 panic
4. 回调中禁止任何可能 panic 的操作（unwrap、expect、assert）

**Pattern**: 音频回调 = 100% panic-free zone

---

### ✅ FIXED: Error::source() 未实现导致错误链丢失 (commit 1834c36)

**位置**: `src/error.rs:18-46` (已修复)  
**问题**: `IoError` 未实现 `std::error::Error::source()`，丢失底层错误信息

**修复前**:
```rust
// ❌ 错误示例：仅保存 ErrorKind，丢失原始错误
pub struct IoError {
    pub source_kind: io::ErrorKind,
}

impl std::error::Error for IoError {}  // source() 返回 None
```

**修复后**:
```rust
// ✅ 正确示例：保存完整错误 + 实现 source()
pub struct IoError {
    pub source: Option<Box<io::Error>>,
}

impl std::error::Error for IoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|e| e.as_ref() as _)
    }
}
```

**教训**:
1. 错误类型必须实现 `Error::source()` 保留错误链
2. 仅保存 `ErrorKind` 会丢失具体信息（文件路径、权限等）
3. 使用 `Box<dyn Error>` 或 `Box<ConcreteError>` 保存原始错误
4. 错误链对于调试至关重要（`error: {}, caused by: {}`）

**Pattern**: 所有自定义 Error 必须实现 `source()` 或使用 `thiserror` derive

---

### ✅ FIXED: async 签名但阻塞 I/O (commit 8edf92c)

**位置**: `src/scrcpy/connection.rs:86` (已修复)  
**问题**: `connect` 签名为 `async fn` 但内部使用阻塞 I/O（`TcpListener::accept`）

**修复前**:
```rust
// ❌ 错误示例：假 async（内部阻塞线程）
pub async fn connect(&mut self) -> Result<()> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let (stream, _) = listener.accept()?;  // 阻塞！
    // ...
}
```

**修复后**:
```rust
// ✅ 正确示例：移除 async，统一为同步 API
pub fn connect(&mut self) -> Result<()> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let (stream, _) = listener.accept()?;  // 明确是同步
    // ...
}
```

**教训**:
1. `async fn` 不代表非阻塞，仅表示可 `.await`
2. 阻塞 I/O 会阻塞整个 executor 线程（tokio/async-std）
3. 要么全部用 `tokio::net`，要么全部用 `std::net`
4. 混用会导致难以追踪的性能问题

**Pattern**: `async fn` 内部禁止 `std::net`、`std::fs`、`std::thread::sleep`

---

### ✅ FIXED: ADB 路径未验证 (commit ec5596e)

**位置**: 多处 `Command::new("adb")` (已修复)  
**问题**: 假设 ADB 在 PATH 中，未验证可执行性

**修复前**:
```rust
// ❌ 错误示例：直接执行，ADB 不存在时产生神秘错误
Command::new("adb").args(&["devices"]).output()?;
```

**修复后**:
```rust
// ✅ 正确示例：启动时验证 + 清晰错误消息
impl AdbShell {
    pub fn verify_adb_available() -> Result<()> {
        Command::new("adb")
            .arg("version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| {
                SAideError::AdbError(format!(
                    "ADB not found in PATH. Please install Android SDK Platform Tools. Error: {}",
                    e
                ))
            })?
            .success()
            .then_some(())
            .ok_or_else(|| {
                SAideError::AdbError(
                    "ADB command failed. Please check Android SDK installation.".to_string(),
                )
            })
    }
}

// main.rs 启动时调用
fn main() -> Result<()> {
    AdbShell::verify_adb_available()?;  // Fail fast
    // ...
}
```

**教训**:
1. 不同系统 ADB 路径不同（Linux: `/usr/bin/adb`, Windows: `platform-tools/adb.exe`）
2. 用户可能未安装 Android SDK
3. 启动时验证外部依赖，fail fast with clear message
4. 错误消息必须告诉用户如何解决（安装 Android SDK Platform Tools）

**Pattern**: 所有外部工具（adb、ffmpeg、scrcpy-server）必须启动时验证

---

### ✅ FIXED: ProjectDirs::from() 在非标准环境 panic (commit d59dca5)

**位置**: `src/constant.rs:11` (已修复)  
**问题**: `ProjectDirs::from(..).expect(..)` 在 Docker/沙盒环境 panic

**修复前**:
```rust
// ❌ 错误示例：Docker 环境无 HOME 变量时 panic
lazy_static! {
    static ref PROJECT_DIRS: ProjectDirs = 
        ProjectDirs::from("com", "SAide", "SAide")
            .expect("Failed to get project directories");
}
```

**修复后**:
```rust
// ✅ 正确示例：3 层 fallback
pub fn config_dir() -> Option<PathBuf> {
    ProjectDirs::from("com", "SAide", "SAide")
        .map(|dirs| dirs.config_dir().to_path_buf())
}

pub fn get_config_path() -> PathBuf {
    config_dir()
        .map(|dir| dir.join("config.toml"))
        .unwrap_or_else(|| PathBuf::from("./config.toml"))  // Fallback 1
}

// src/config/mod.rs 中再添加 Fallback 2
let path = get_config_path();
if !path.exists() {
    return Config::default();  // 使用默认配置
}
```

**教训**:
1. Docker/CI 环境可能没有 `HOME`、`XDG_CONFIG_HOME` 等变量
2. 必须提供多层 fallback（user dir → ./config.toml → /tmp/saide）
3. 配置文件不存在时使用默认配置，而非 panic
4. 避免 `lazy_static!` + `expect()` 组合（初始化时 panic 难以调试）

**Pattern**: 配置路径 = 用户目录 → 当前目录 → 临时目录 → 默认值

---

### ✅ FIXED: 非 UTF-8 路径 panic (commit 23ea5f7)

**位置**: `src/config/mod.rs:121` (已修复)  
**问题**: `path.to_str().unwrap()` 在 Windows 特殊字符路径 panic

**修复前**:
```rust
// ❌ 错误示例：Windows 日文/中文路径包含非 UTF-8 字符时 panic
let path_str = path.to_str().unwrap();
```

**修复后**:
```rust
// ✅ 正确示例：使用 to_string_lossy() 容忍非 UTF-8
let path_str = path.to_string_lossy();
```

**教训**:
1. Windows 路径可能包含非 UTF-8 字符（特殊语言、emoji）
2. `to_str()` 返回 `Option<&str>`，遇到非 UTF-8 返回 `None`
3. `to_string_lossy()` 将非 UTF-8 字节替换为 `�`（U+FFFD）
4. 路径显示场景（日志、错误消息）应优先使用 `to_string_lossy()`

**Pattern**: 路径转字符串 = `to_string_lossy()` > `to_str().ok_or(...)` >> `to_str().unwrap()`

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

### ✅ FIXED: 假设 ADB 在 PATH 中 (commit ec5596e)

**位置**: `src/controller/adb.rs`, `src/scrcpy/server.rs` (已修复)  
**问题**: 多处 `Command::new("adb")` 未验证可执行性

**教训**:
1. 不同系统 ADB 路径不同（Linux: `/usr/bin/adb`, Windows: `platform-tools/adb.exe`）
2. 用户可能未安装 Android SDK
3. 应在启动时验证并缓存 ADB 路径

**正确做法**:
```rust
// ✅ 启动时验证（已实现）
impl AdbShell {
    pub fn verify_adb_available() -> Result<()> {
        Command::new("adb")
            .arg("version")
            .status()
            .map_err(|_| SAideError::AdbError("ADB not found in PATH. Please install Android SDK Platform Tools.".into()))?;
        Ok(())
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

### ✅ FIXED: ProjectDirs panic in Docker/CI (commit d59dca5)

**位置**: `src/constant.rs:11` (已修复)  
**问题**: `ProjectDirs::from(...).expect()` 在 Docker/CI 环境（无 HOME 变量）panic

**修复前**:
```rust
// ❌ 错误示例：Docker 环境立即崩溃
lazy_static! {
    static ref PROJECT_DIR: ProjectDirs =
        ProjectDirs::from("io", "keivry", "saide")
            .expect("Failed to determine project directories");
}
```

**修复后**:
```rust
// ✅ 正确示例：3 层降级策略
pub fn config_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("io", "keivry", "saide")
        .map(|dirs| dirs.config_dir().join("config.toml"))
}

// ConfigManager::new() 中:
let path = constant::config_dir().unwrap_or_else(|| {
    warn!("Unable to determine config directory, using fallback: /tmp/saide");
    constant::fallback_config_path()  // /tmp/saide/config.toml
});
```

**教训**:
1. **永远不要在 `lazy_static!` 中 `.expect()`** - 它在程序启动时就会评估
2. 环境变量（HOME, XDG_CONFIG_HOME）在 Docker/CI/sandbox 可能不存在
3. 必须提供降级路径：用户目录 → 当前目录 → 临时目录
4. Docker 用户应挂载卷持久化配置：`docker run -v /host/config:/tmp/saide`

**相关**: commit 23ea5f7 (非 UTF-8 路径修复)

---

### ✅ FIXED: 非 UTF-8 路径 panic (commit 23ea5f7)

**位置**: `src/config/mod.rs:122` (已修复)  
**问题**: `path.to_str().unwrap()` 在 Windows 特殊字符路径 panic

**修复前**:
```rust
// ❌ 错误示例：Windows 用户名含中文/特殊字符时崩溃
let path = dir.data_dir().join("scrcpy-server");
if path.is_file() {
    return path.to_str().unwrap().to_string();  // panic!
}
```

**修复后**:
```rust
// ✅ 正确示例：to_string_lossy 处理无效 UTF-8
if path.is_file() {
    return path.to_string_lossy().to_string();  // 替换无效字符为 �
}
```

**教训**:
1. Windows 路径可能包含非 Unicode 字符（旧版编码遗留）
2. `to_str()` 返回 `Option<&str>`，遇到非 UTF-8 返回 `None`
3. `to_string_lossy()` 返回 `Cow<str>`，保证不 panic
4. 文件路径必须用 `to_string_lossy()`，绝不能 `unwrap()`

**相关**: `std::path::Path` 文档中的 UTF-8 注意事项

---

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
