# SAide 开发陷阱记录

> **目的**: 记录开发过程中遇到的坑、反模式、失败案例,确保不再重复犯错

---

## 坐标系统陷阱 (P1 优先级)

> **2026-01-22 更新**: 新增自动视频旋转补偿功能 (capture_orientation 锁定时)  
> **2026-01-22 更新**: 修复按键映射在设备旋转后坐标错误的 bug

### ✅ FIXED: ControlSender 分辨率未同步导致按键坐标错误

**位置**: `src/saide/ui/saide.rs:1049-1070` (已修复)  
**问题**: 设备旋转导致视频分辨率变化,但 `ControlSender` 的 `screen_size` 未更新,导致按键映射坐标计算错误

**症状**:

- Gentoo + KDE Plasma 6: 正常工作
- Ubuntu 22.04 + GNOME 42: 按键发送但设备无响应
- 日志显示坐标错误:
  - 正常: `ScrcpyPos(1186, 528)` (对应横屏 1280x576)
  - 异常: `ScrcpyPos(534, 1173)` (使用竖屏 576x1280 计算)

**根本原因**:

```rust
// KeyboardMapper.apply_active_profile() 使用 ControlSender 的 screen_size
let (video_width, video_height) = self.sender.get_screen_size();  // ← 旧分辨率!

// 坐标计算
x = 0.9275 * video_width   // 0.9275 * 576 = 534  (错误!)
y = 0.654 * video_height   // 0.654 * 1280 = 837  (错误!)

// 正确应该是:
// x = 0.9275 * 1280 = 1187
// y = 0.654 * 576 = 377
```

**为什么 Gentoo 正常,Ubuntu 异常?**

`ControlSender.update_screen_size()` 只在**鼠标事件处理时**被调用:

```rust
// src/saide/ui/saide.rs:597-600, 638-641
fn process_mouse_button_event(...) {
    sender.update_screen_size(...);  // ✅ 鼠标事件触发频繁
}
```

- **Gentoo**: 用户可能移动了鼠标 → 触发 `update_screen_size()` → 按键坐标正确
- **Ubuntu**: 未移动鼠标 → `ControlSender` 仍使用旧分辨率 → 按键坐标错误

**修复后**:

```rust
// src/saide/ui/saide.rs:1049-1070
// 视频分辨率变化时
self.app_state.scrcpy_coords_mut()
    .update_video_size(new_dimensions.0 as u16, new_dimensions.1 as u16);

// ✅ 同步更新 ControlSender 的 screen_size
if let Some(sender) = &self.app_state.control_sender {
    sender.update_screen_size(new_dimensions.0 as u16, new_dimensions.1 as u16);
}

// ✅ 重新应用按键映射,使用新分辨率计算坐标
if let Some(keyboard_mapper) = &self.app_state.keyboard_mapper {
    keyboard_mapper.apply_active_profile();
}
```

**教训**:

1. **状态同步陷阱**: 同一信息(视频分辨率)存储在多处时,必须确保所有副本同步更新
   - `ScrcpyCoordSys.video_width/height`
   - `ControlSender.screen_size`
   - `KeyboardMapper.scrcpy_mappings` (依赖前两者)
2. **隐式依赖**: `update_screen_size()` 在鼠标事件中调用,隐式假设"用户会移动鼠标",纯键盘操作时失效
3. **跨平台差异**: 用户行为习惯(Gentoo 用户可能更常用鼠标)导致 bug 在不同环境下表现不同
4. **调试技巧**: 当坐标值明显错误时,反推分辨率可快速定位问题根源
   - `534 / 0.9275 = 576` → 立即发现使用了竖屏宽度

**相关代码**:

- 坐标计算: `src/controller/keyboard.rs:362-366`
- ControlSender: `src/controller/control_sender.rs:27-41`
- 分辨率更新: `src/saide/ui/saide.rs:1036-1070`

---

### ✅ NEW: 自动视频旋转补偿 (2026-01-22 实现)

**位置**: `src/saide/ui/saide.rs:446-480` (新增)  
**功能**: 当 `capture_orientation` 启用时（锁定视频捕获方向），设备物理旋转后自动调整 `video_rotation` 以保持画面正确显示

**核心逻辑**:

```rust
fn apply_auto_rotation(&mut self, ctx: &egui::Context) {
    if let Some(capture_orient) = self.app_state.scrcpy_coords().capture_orientation {
        let device_orient = self.app_state.device_orientation();

        let target_rotation = 4 - ((capture_orient + device_orient) % 4);

        self.player.set_rotation(target_rotation);
        self.indicator.update_video_rotation(target_rotation);
        self.ui_state
            .visual_coords_mut()
            .update_rotation(target_rotation);

        self.resize(ctx);
        self.indicator.update_video_resolution(self.player.video_dimensions());
    }
}
```

**触发时机**: `DeviceMonitorEvent::Rotated` 事件处理时 (`process_device_monitor_events()`)

**旋转公式推导** (重要！):

```
问题：设备从竖屏(0°)旋转到横屏(90°CW)，capture锁定在竖屏(0°)，视频如何旋转？

情景：
1. capture_orientation=0 → 视频捕获方向锁定在竖屏
2. device_orientation=1 → 设备物理向右旋转90°（用户横屏握持）
3. 视频内容仍然是竖屏画面（因为capture锁定）
4. 用户横屏握持设备，看到竖屏内容需要逆时针旋转270°才能正确显示

答案：video_rotation = (capture_orientation - device_orientation + 4) % 4
                      = (0 - 1 + 4) % 4 = 3 (270°CW = 90°CCW)

关键理解：
- 按键映射坐标转换: (device + capture) % 4
  → 从设备方向坐标转换到捕获方向坐标（顺时针旋转）
- 视频显示旋转: (capture - device + 4) % 4
  → 从捕获方向画面转换到设备方向显示（逆时针旋转，相反方向）

验证：
- device=0, capture=0: video_rotation = (0-0+4)%4 = 0 ✓ (竖屏对竖屏，无需旋转)
- device=1, capture=0: video_rotation = (0-1+4)%4 = 3 ✓ (横屏看竖屏，逆转270°)
- device=3, capture=0: video_rotation = (0-3+4)%4 = 1 ✓ (横屏看竖屏，逆转90°)

错误公式（已修复3次）：
❌ rotation = (device - capture_cw) % 4  // 第一次错误
❌ rotation = (device + capture) % 4     // 第二次错误（与映射混淆）
✅ rotation = (capture - device + 4) % 4 // 正确！相反方向
```

**窗口自动调整**:

- 旋转后调用 `resize(ctx)` 自动调整窗口尺寸
- 横屏↔竖屏切换时窗口宽高互换
- 配合 `smart_window_resize` 确保窗口适配屏幕

**启用条件** (避免不必要的锁定):

```rust
// src/scrcpy/server.rs:141-150
pub fn should_lock_orientation_for_nvdec(hwdecode: bool) -> bool {
    if !hwdecode {
        return false;  // 软解码不锁定
    }
    if let GpuType::Nvidia = detect_gpu() {
        return true;   // NVDEC 硬解码锁定
    }
    false
}
```

**使用场景**:

1. **NVIDIA GPU + hwdecode=true**: 自动启用 `capture_orientation=0` (锁定竖屏)
2. **手动配置**: `config.toml` 中设置 `[scrcpy.video] capture_orientation = 0`
3. **软解码 (hwdecode=false)**: 不锁定，视频随设备旋转

**配置示例**:

```toml
[gpu]
hwdecode = true  # NVIDIA GPU 会自动锁定 capture_orientation

[scrcpy.video]
# 手动锁定方向 (可选)
# capture_orientation = 0  # 0=portrait, 1=landscape90CCW, 2=portrait180, 3=landscape270CCW
```

**教训**:

1. **公式反向性**: 视频旋转公式是按键映射公式的**逆运算** (`capture - device` vs `device + capture`)
   - 按键映射: 坐标从设备方向转到捕获方向
   - 视频显示: 画面从捕获方向转到设备方向（相反）
2. **窗口自动调整**: 旋转后必须调用 `resize(ctx)` 重新计算窗口尺寸，否则横竖屏切换时窗口大小错误
3. **条件判断**: `should_lock_orientation_for_nvdec()` 必须同时检查 `hwdecode` 和 GPU 类型，避免软解码时不必要的锁定
4. **状态同步**: 旋转后需同步更新:
   - `StreamPlayer.rotation`
   - `Indicator.video_rotation` + `video_resolution`
   - `VisualCoordSys.rotation`
5. **易错点**: 这个公式**非常容易理解错**（已错3次），必须有详细注释防止"修复"成错误代码

**修复的 Bug**:

1. ❌ **90°/270°上下颠倒**:
   - 第1次错误: `(device - capture_cw) % 4` (CCW转换错误)
   - 第2次错误: `(device + capture) % 4` (与按键映射混淆，方向相反)
   - ✅ 最终修复: `(capture - device + 4) % 4` (视频旋转是映射的逆运算)
2. ❌ **旋转时窗口未调整**: 缺少 `resize(ctx)` 调用
   - ✅ 修复: 旋转后立即调用 `resize()` 和更新 `indicator`
3. ❌ **软解码时仍锁定**: `should_lock_orientation_for_nvdec()` 未检查 `hwdecode`
   - ✅ 修复: 新增 `hwdecode: bool` 参数，软解码时返回 `false`

**相关代码**:

- 自动旋转: `src/saide/ui/saide.rs:446-480`
- 旋转公式参考: `src/saide/coords/mapping.rs:54` (按键映射坐标转换)
- 启用条件: `src/scrcpy/server.rs:141-150`
- 设备监控: `src/saide/device_monitor.rs:146-188`

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

## i18n 性能陷阱

> **2026-01-21 更新**: 修复 i18n 宏在 60fps UI 中的 RwLock 竞争

### ✅ FIXED: t!/tf! 宏在每帧获取锁 (commit PENDING)

**位置**: `src/i18n/mod.rs:41-71` (已修复)  
**问题**: egui 60fps 渲染时,每个 UI 文本每帧调用 `RwLock.read()`,导致严重锁竞争

**修复前**:

```rust
// ❌ 错误示例: 每帧每个 UI 文本都获取锁
macro_rules! t {
    ($key:expr) => {
        $crate::i18n::L10N.read().get($key)  // 60fps × 20 texts = 1200 locks/sec
    };
}

// UI 代码 (每秒重绘 60 次):
fn update(&mut self, ctx: &egui::Context) {
    ui.label(t!("indicator-panel-fps"));      // Lock 1
    ui.label(t!("indicator-panel-resolution")); // Lock 2
    ui.label(t!("mapping-config-title"));      // Lock 3
    // ... 17 more locks per frame
}
```

**性能影响**:

```
20 个 UI 文本 × 60fps = 1200 次 RwLock.read()/秒
├─ RwLock 开销: ~50-100ns (无竞争时)
├─ HashMap 查找: ~20-50ns
├─ Fluent 格式化: ~500-1000ns (带参数时)
└─ 总计: ~60-120μs/帧 → 可能达到 1ms (1.67% CPU @ 60fps)
```

**修复后**:

```rust
// ✅ 正确示例: 检查缓存后再获取锁
macro_rules! t {
    ($key:expr) => {{
        if let Some(cached) = $crate::i18n::get_cached($key) {
            cached  // ✅ 缓存命中 - 无锁访问
        } else {
            let result = $crate::i18n::L10N.read().get($key);  // 缓存未命中 - 获取锁
            $crate::i18n::set_cached($key.to_string(), result.clone());
            result
        }
    }};
}

// 支持代码: thread-local LRU 缓存
thread_local! {
    static TRANSLATION_CACHE: RefCell<TranslationCache> =
        RefCell::new(TranslationCache::new(256));  // 256 条缓存
}

struct TranslationCache {
    cache: lru::LruCache<String, String>,
    generation: u64,  // 检测 locale 切换
}

impl TranslationCache {
    fn get(&mut self, key: &str, current_gen: u64) -> Option<String> {
        if self.generation != current_gen {
            self.cache.clear();  // Locale 切换,清空缓存
            self.generation = current_gen;
            return None;
        }
        self.cache.get(key).cloned()
    }
}

// 全局 generation 计数器 (无锁检查)
static CACHE_GENERATION: AtomicU64 = AtomicU64::new(0);

pub fn set_locale(locale: &str) {
    // ... 切换 locale ...
    CACHE_GENERATION.fetch_add(1, Ordering::Relaxed);  // 失效所有缓存
}
```

**性能改进**:

```
首次调用 t!("key"):
├─ get_cached("key") → None (缓存未命中)
├─ L10N.read().get("key") → "Translation" (获取锁)
└─ set_cached("key", "Translation") → 写入缓存

后续调用 (同一帧/后续帧):
├─ get_cached("key") → Some("Translation") (缓存命中)
└─ 直接返回 → **无锁访问**

性能对比:
- 修复前: 1200 locks/sec (60fps × 20 texts)
- 修复后: ~60 locks/sec (仅缓存未命中时)
- **改进**: 20× 减少锁竞争
```

**教训**:

1. **高频调用路径必须避免不必要的锁** - 60fps UI 每秒执行数千次
2. **thread-local 缓存 > 全局缓存** - 避免多线程竞争 (egui 单线程渲染)
3. **LRU 缓存自动淘汰** - 避免内存泄漏 (限制 256 条)
4. **generation 计数器实现缓存失效** - locale 切换时清空缓存
5. **缓存键设计**:
   - `t!("key")` → cache key = `"key"` (简单)
   - `tf!("key", "arg" => val)` → cache key = `"key:{:?}args"` (包含参数)
6. **避免在宏展开前获取锁** - 必须先检查缓存再决定是否锁

**Pattern**: 高频 API = 检查缓存 → (未命中) → 获取锁 → 更新缓存

**相关优化**:

- LRU 缓存容量: 256 条 (足够覆盖所有 UI 文本)
- `AtomicU64` generation: Relaxed ordering (无需强一致性)
- `RefCell` 缓存: 单线程渲染无需 `Mutex`

**潜在问题**:

1. ⚠️ `tf!()` 缓存键生成开销: `format!("{}:{:?}", key, args)` 可能较慢
   - 解决: 参数较少时可接受,复杂参数考虑禁用缓存
2. ⚠️ 缓存内存占用: 256 × 平均 50 字节 = ~12KB/线程
   - 解决: LRU 自动淘汰,影响可忽略

**调试验证**:

```bash
# 编译检查
cargo clippy -- -D warnings

# 运行测试
cargo test --lib i18n

# 性能测试 (可选):
# 在 update() 中添加 eprintln!("Lock count: {}", lock_count);
# 预期: 首次渲染 ~20 locks, 后续 ~0-2 locks/frame
```

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

| 平台                    | 推荐值 (frames) | 延迟 @ 48kHz | 说明           |
| ----------------------- | --------------- | ------------ | -------------- |
| 树莓派 / 低功耗         | 256-512         | 5.33-10.67ms | CPU 弱，解码慢 |
| 桌面 Linux (PulseAudio) | 128-256         | 2.67-5.33ms  | 混音器开销     |
| 桌面 Linux (PipeWire)   | 64-128          | 1.33-2.67ms  | 低延迟混音器   |
| Windows (WASAPI shared) | 128-256         | 2.67-5.33ms  | 共享模式开销   |
| macOS (CoreAudio)       | 64-128          | 1.33-2.67ms  | 硬件性能优秀   |

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
Phase 3: 64 frames (1.33ms)  - 低延迟优先,可配置回退
```

---

## FFI 互操作陷阱 (P1 优先级)

> **2026-01-22 更新**: 修复 VAAPI 设备路径传递给 FFmpeg 时出现乱码

### ✅ FIXED: Rust &str 传给 C API 必须转换为 CString

**位置**: `src/decoder/vaapi.rs:117` (已修复)  
**问题**: 直接使用 `as_ptr()` 传递 Rust `&str` 给 FFmpeg C API,导致设备路径后出现乱码

**修复前**:

```rust
// ❌ 错误示例: &str 不保证 null-terminated
let device_path_cstr = device_path.to_str().unwrap();
unsafe {
    ffmpeg::sys::av_hwdevice_ctx_create(
        &mut hw_device_ctx,
        ffmpeg::sys::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI,
        device_path_cstr.as_ptr() as *const std::os::raw::c_char, // ❌ 未 null-terminated
        ptr::null_mut(),
        0,
    );
}
// 结果: FFmpeg 读到 "/dev/dri/renderD128<garbage>"
```

**修复后**:

```rust
// ✅ 正确示例: CString 保证 null-terminated
use std::ffi::CString;

let device_path_cstr = device_path.to_str().unwrap();
let device_path_c = CString::new(device_path_cstr).map_err(|e| {
    VideoError::InitializationError(format!(
        "VAAPI device path contains null byte: {e}"
    ))
})?;

unsafe {
    ffmpeg::sys::av_hwdevice_ctx_create(
        &mut hw_device_ctx,
        ffmpeg::sys::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI,
        device_path_c.as_ptr(),  // ✅ 保证 null-terminated
        ptr::null_mut(),
        0,
    );
}
```

**根本原因**:

- Rust `&str` 是长度+指针表示,**不包含** null 结尾符 (`\0`)
- C 字符串必须以 `\0` 结尾,否则读取时会越界访问内存
- `CString::new()` 会自动追加 `\0`,并在路径包含内部 null 字节时返回错误

**适用范围**:
所有传递给 C FFI 的字符串参数都必须使用 `CString`:

```rust
// ✅ 正确模式
use std::ffi::CString;

// 1. 路径传递
let path = CString::new("/dev/dri/renderD128").unwrap();
unsafe_c_function(path.as_ptr());

// 2. 错误处理 null 字节
let user_input = "/path\0with\0nulls";
match CString::new(user_input) {
    Ok(c_str) => unsafe_c_function(c_str.as_ptr()),
    Err(e) => eprintln!("Invalid input: {}", e),
}

// 3. 字符串生命周期
let path = CString::new("/tmp/file").unwrap();
// ❌ 错误: CString 被 drop,指针悬空
unsafe_c_function(path.as_ptr());
drop(path);

// ✅ 正确: 确保 CString 活到 FFI 调用结束
let path = CString::new("/tmp/file").unwrap();
unsafe_c_function(path.as_ptr());
// path 在这里才被 drop
```

**性能影响**:

- `CString::new()` 会分配堆内存并复制字符串 → **一次性开销**,可忽略
- 如果频繁调用,可考虑缓存 `CString` 对象

**参考资料**:

- [Rust FFI Guide: Strings](https://doc.rust-lang.org/nomicon/ffi.html#strings)
- [std::ffi::CString](https://doc.rust-lang.org/std/ffi/struct.CString.html)

---

### ✅ FIXED: FFmpeg 帧数据直接拷贝导致颜色乱码

**位置**: `src/decoder/h264.rs:163` (已修复)  
**问题**: 软件解码器直接使用 `rgb_frame.data(0).to_vec()` 拷贝 RGBA 数据,未考虑 FFmpeg linesize padding

**症状**:

- VAAPI 失败 fallback 到软件解码时,画面颜色乱七八糟
- 每行像素错位,产生斜纹/错位效果
- 分辨率越大,错位越明显

**修复前**:

```rust
// ❌ 错误示例: 直接拷贝包含 padding 的数据
let mut rgb_frame = VideoFrame::empty();
scaler.run(&decoded, &mut rgb_frame)?;

let data = rgb_frame.data(0).to_vec(); // ❌ 包含 linesize padding

frames.push(DecodedFrame {
    width: self.width,
    height: self.height,
    data,  // ❌ 传给 wgpu 的数据行宽不匹配
    pts,
    format: Pixel::RGBA,
});
```

**修复后**:

```rust
// ✅ 正确示例: 逐行拷贝移除 padding
let linesize = rgb_frame.stride(0);
let width = self.width as usize;
let height = self.height as usize;
let bytes_per_pixel = 4;

let expected_stride = width * bytes_per_pixel;
let data = if linesize == expected_stride {
    // No padding - direct copy
    rgb_frame.data(0)[0..(width * height * bytes_per_pixel)].to_vec()
} else {
    // Has padding - copy line by line
    let mut data = Vec::with_capacity(width * height * bytes_per_pixel);
    let src = rgb_frame.data(0);
    for row in 0..height {
        let start = row * linesize;
        let end = start + expected_stride;
        data.extend_from_slice(&src[start..end]);
    }
    data
};
```

**根本原因**:

- FFmpeg 的 `AVFrame` 使用 **linesize** 表示每行字节数,通常会 **对齐到 32/64 字节边界**
- 例如: 864×1920 RGBA 图像
  - 理论行宽: `864 × 4 = 3456 bytes`
  - 实际 linesize: `3488 bytes` (对齐到 32 字节 → 3456 + 32 padding)
- 直接拷贝会包含 padding,传给 wgpu 时 `bytes_per_row` 不匹配,导致每行数据错位

**为什么 VAAPI 没问题?**
VAAPI 解码器早在 commit `06abfd0` 就修复了此问题:

```rust
// VAAPI: 已修复 (2025-12-11)
let y_linesize = sw_frame.stride(0);
for row in 0..height {
    let start = row * y_linesize;
    let end = start + width;
    data.extend_from_slice(&y_data[start..end]);
}
```

但 H.264 软件解码器**未同步修复**,导致 fallback 时出现颜色乱码。

**性能优化**:

- 当 `linesize == expected_stride` 时(无 padding),使用快速路径直接拷贝
- 仅在有 padding 时才逐行拷贝,避免不必要的性能损耗

**适用范围**:
所有从 FFmpeg `AVFrame` 提取数据的场景都需检查 linesize:

```rust
// ✅ 通用模式
let linesize = frame.stride(plane_index);
let width = frame.width() as usize;
let height = frame.height() as usize;
let bytes_per_pixel = /* 根据格式计算 */;

let expected_stride = width * bytes_per_pixel;
if linesize != expected_stride {
    // 逐行拷贝移除 padding
    for row in 0..height {
        let start = row * linesize;
        let end = start + expected_stride;
        // ...
    }
}
```

**参考资料**:

- [FFmpeg AVFrame linesize 文档](https://ffmpeg.org/doxygen/trunk/structAVFrame.html#a2c5d080a18c4ba0af9c8da4d34f9e3e8)
- commit `06abfd0`: VAAPI NV12 linesize 修复

---

## 窗口管理陷阱 (P1 优先级)

> **2026-01-22 更新**: 修复 GNOME 等桌面环境窗口大小限制问题

### ✅ FIXED: WM 限制导致窗口无法按视频分辨率调整

**位置**: `src/saide/ui/saide.rs:231-311` (已修复)  
**问题**: GNOME/Wayland 等 WM 会限制窗口最大尺寸,直接按视频分辨率调整窗口可能失败

**症状**:

- 视频分辨率 1080×2400 时,窗口无法完全显示
- WM 强制缩小窗口至屏幕可用范围
- 视频显示区域被裁剪或压缩变形

**修复前**:

```rust
fn resize(&mut self, ctx: &egui::Context) {
    let (w, h) = self.player.video_dimensions();
    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
        w as f32 + Toolbar::width(),
        h as f32,
    )));
}
```

**修复后**:

```rust
fn resize(&mut self, ctx: &egui::Context) {
    let (video_w, video_h) = self.player.video_dimensions();
    let config = self.config_state.config();
    let smart_resize = config.general.smart_window_resize;

    let (window_w, window_h) = if smart_resize {
        let screen_rect = ctx.input(|i| i.viewport().monitor_size);

        if let Some(monitor_size) = screen_rect {
            Self::calculate_window_size(
                video_w, video_h,
                monitor_size.x, monitor_size.y
            )
        } else {
            (video_w, video_h)
        }
    } else {
        (video_w, video_h)
    };

    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(...));
}

fn calculate_window_size(
    video_w: u32, video_h: u32,
    screen_w: f32, screen_h: f32,
) -> (u32, u32) {
    const SCREEN_MARGIN_RATIO: f32 = 0.9;

    let usable_w = (screen_w * SCREEN_MARGIN_RATIO) as u32;
    let usable_h = (screen_h * SCREEN_MARGIN_RATIO) as u32;

    if video_w <= usable_w && video_h <= usable_h {
        return (video_w, video_h);
    }

    let video_long = video_w.max(video_h);

    for &tier in VIDEO_RESOLUTION_TIERS {
        if tier >= video_long { continue; }

        let scale = tier as f32 / video_long as f32;
        let (scaled_w, scaled_h) = /* 计算缩放后尺寸 */;

        if scaled_w <= usable_w && scaled_h <= usable_h {
            return (scaled_w, scaled_h);
        }
    }

    /* 使用最小档位缩放 */
}
```

**核心策略**:

1. **预置分辨率档位** (定义在 `constant.rs`):

   ```rust
   pub const VIDEO_RESOLUTION_TIERS: &[u32] = &[
       3840, // 4K UHD
       2560, // QHD / 1440p
       1920, // FHD / 1080p
       1600, // HD+
       1280, // HD / 720p
       960,  // qHD
       800,  // SVGA
       640,  // VGA
       480,  // HVGA
   ];
   ```

2. **智能档位选择**:
   - 视频两个维度都 ≤ 屏幕 90% → 使用原始分辨率
   - 某维度超出 → 向下查找最近档位,按比例缩放
   - 保证宽高比不变

3. **配置开关** (`config.toml`):
   ```toml
   [general]
   smart_window_resize = true  # 默认启用
   ```

**教训**:

1. **不同桌面环境 WM 行为差异**:
   - KDE Plasma: 允许窗口超出屏幕,用户自行滚动
   - GNOME/Wayland: 强制限制窗口最大尺寸
   - macOS: 限制窗口但允许全屏
2. **egui 提供屏幕尺寸检测**: `ctx.input(|i| i.viewport().monitor_size)`
3. **档位设计原则**:
   - 覆盖常见分辨率(480p-4K)
   - 降序排列便于搜索
   - 使用长边值(适配横竖屏)
4. **用户可控**: 提供配置开关,允许禁用智能缩放

**性能影响**:

- `calculate_window_size()` 仅在窗口调整时调用(旋转/首帧)
- 档位查找 O(n),n=9 可忽略
- 无额外内存开销

**相关代码**:

- 常量定义: `src/constant.rs:64-76`
- 窗口调整: `src/saide/ui/saide.rs:231-311`
- 配置项: `src/config/mod.rs:143-148`

**调试技巧**:

```bash
# 查看实际使用的窗口尺寸
RUST_LOG=debug cargo run 2>&1 | grep "InnerSize"

# 测试不同屏幕尺寸
# 1. 修改 monitor_size mock (开发环境)
# 2. 实际在 1080p/4K 显示器测试
```

**已知限制**:

- 多显示器环境可能获取到错误的显示器尺寸 (egui 限制)
- Wayland 某些合成器可能不提供 monitor_size

---

## 解码器选择陷阱 (P1 优先级)

> **2026-01-23 更新**: 移除 GPU 检测依赖，实现级联降级策略

### ✅ FIXED: GPU 检测导致解码器选择失败

**位置**: `src/decoder/auto.rs`, `src/scrcpy/codec_probe.rs` (已修复)  
**问题**: 依赖 GPU 检测选择解码器，在多 GPU 系统或 GPU 检测失败时无法使用正确解码器

**修复前**:

```rust
pub fn new(width: u32, height: u32, hwdecode: bool) -> Result<Self> {
    if !hwdecode {
        return Ok(Self::Software(H264Decoder::new(width, height)?));
    }

    let gpu_type = detect_gpu();
    info!("Detected GPU type: {:?}", gpu_type);

    match gpu_type {
        GpuType::Nvidia => {
            info!("Attempting NVIDIA NVDEC hardware decoder");
            NvdecDecoder::new(width, height)
                .map(Self::Nvdec)
                .or_else(|e| {
                    warn!("NVDEC initialization failed: {}", e);
                    info!("Falling back to software H.264 decoder");
                    Ok(Self::Software(H264Decoder::new(width, height)?))
                })
        }
        GpuType::Intel | GpuType::Amd => {
            #[cfg(not(target_os = "windows"))]
            {
                info!("Attempting Linux VAAPI hardware decoder");
                VaapiDecoder::new(width, height)
                    .map(Self::Vaapi)
                    .or_else(|e| {
                        warn!("VAAPI initialization failed: {}", e);
                        info!("Falling back to software H.264 decoder");
                        Ok(Self::Software(H264Decoder::new(width, height)?))
                    })
            }
            #[cfg(target_os = "windows")]
            {
                info!("Attempting Windows D3D11VA hardware decoder");
                D3d11vaDecoder::new(width, height)
                    .map(Self::D3d11va)
                    .or_else(|e| {
                        warn!("D3D11VA initialization failed: {}", e);
                        info!("Falling back to software H.264 decoder");
                        Ok(Self::Software(H264Decoder::new(width, height)?))
                    })
            }
        }
        _ => {
            info!("Unknown or unsupported GPU, using software decoder");
            Ok(Self::Software(H264Decoder::new(width, height)?))
        }
    }
}
```

**问题**:

1. **多 GPU 系统跳过可用解码器**:
   - Intel iGPU + NVIDIA dGPU 系统: 检测到 Intel → 不尝试 NVDEC
   - AMD APU + NVIDIA dGPU 系统: 检测到 AMD → 不尝试 NVDEC

2. **GPU 检测失败导致软解码**:
   - Windows DXGI 未实现 → 返回 `GpuType::Unknown` → 直接软解码
   - `/proc/driver/nvidia` 不存在 → 漏检 NVIDIA GPU

3. **codec_probe 依赖 GPU 类型选择 profile**:
   ```rust
   let gpu_type = detect_gpu();
   let profile_option = get_profile_for_gpu(gpu_type);
   info!("Detected GPU: {:?}, using {}={}", gpu_type, profile_option.0, profile_option.1);
   
   let mut candidate_options: Vec<(&str, &str)> = vec![profile_option];
   ```
   - NVIDIA GPU 检测失败 → 使用 Baseline profile (66) → NVDEC 性能下降
   - Intel GPU 误检测为 Unknown → 强制 Baseline → 无法利用 VAAPI 高级 profile

**修复后** (Cascade Fallback 策略):

```rust
pub fn new(width: u32, height: u32, hwdecode: bool) -> Result<Self> {
    if !hwdecode {
        return Ok(Self::Software(H264Decoder::new(width, height)?));
    }

    info!("Starting cascade decoder selection (NVDEC → VAAPI/D3D11VA → Software)");

    if let Ok(decoder) = NvdecDecoder::new(width, height) {
        info!("✅ Using NVIDIA NVDEC hardware decoder");
        return Ok(Self::Nvdec(decoder));
    }
    warn!("NVDEC unavailable, trying VAAPI/D3D11VA");

    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(decoder) = VaapiDecoder::new(width, height) {
            info!("✅ Using Linux VAAPI hardware decoder");
            return Ok(Self::Vaapi(decoder));
        }
        warn!("VAAPI unavailable, falling back to software decoder");
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(decoder) = D3d11vaDecoder::new(width, height) {
            info!("✅ Using Windows D3D11VA hardware decoder");
            return Ok(Self::D3d11va(decoder));
        }
        warn!("D3D11VA unavailable, falling back to software decoder");
    }

    info!("Using software H.264 decoder");
    Ok(Self::Software(H264Decoder::new(width, height)?))
}
```

```rust
pub fn probe_device(serial: &str, server_jar: &str) -> Result<Option<String>> {
    info!("🔍 Probing codec compatibility for device: {}", serial);
    let mut profile = DeviceProfile::new(serial)?;

    info!("Starting cascade profile testing (NVDEC → Baseline)...");
    
    for (key, value) in CODEC_PROFILES {
        info!("Testing {}={}...", key, value);
        let options = format!("{}={}", key, value);
        if test_codec_options(serial, server_jar, &options, profile.video_encoder.as_deref())? {
            info!("✅ Profile {}={} supported", key, value);
            profile.supported_profile = Some(value.to_string());
            break;
        } else {
            info!("❌ Profile {}={} not supported", key, value);
        }
    }

    let candidate_options: Vec<(&str, &str)> = CODEC_OPTIONS_BASE.iter()
        .filter(|(key, _)| match *key {
            "latency" if profile.android_version < 11 => false,
            "max-bframes" if profile.android_version < 13 => false,
            _ => true,
        })
        .copied()
        .collect();

    info!("Testing {} codec options...", candidate_options.len());
    // ... test each option
}
```

**优势**:

1. **多 GPU 系统自动选择最佳解码器**:
   - Intel iGPU + NVIDIA dGPU: 尝试 NVDEC → 成功使用 dGPU
   - AMD APU + NVIDIA dGPU: 尝试 NVDEC → 成功使用 dGPU
   - 仅 Intel iGPU: NVDEC 失败 → VAAPI 成功 → 使用 iGPU

2. **GPU 检测失败仍能使用硬件加速**:
   - Windows DXGI 未实现 → 仍会尝试 NVDEC + D3D11VA
   - `/proc/driver/nvidia` 不存在 → 仍会尝试 NVDEC (FFmpeg 内部检测)

3. **Codec profile 自动测试**:
   - 先测试 NVDEC profile (65536) → 成功则使用
   - NVDEC 失败 → 测试 Baseline profile (66) → Fallback
   - 设备实际验证,不依赖 GPU 检测猜测

4. **跨平台统一策略**:
   - Linux: NVDEC → VAAPI → Software
   - Windows: NVDEC → D3D11VA → Software
   - 相同逻辑,仅平台特定解码器不同

**教训**:

1. **依赖检测不如直接尝试**: FFmpeg 解码器内部已实现硬件检测,无需在外部再次检测
2. **多 GPU 系统需要级联尝试**: 单一 GPU 检测无法反映所有可用硬件
3. **Fallback 策略优于分支判断**: `try_nvdec() || try_vaapi() || try_software()` 比 `if gpu == nvidia { nvdec } else { ... }` 更健壮
4. **测试真实行为而非猜测能力**: Profile 测试在设备上运行,比猜测 GPU 类型更准确

**保留的 GPU 检测使用** (非关键路径):

`src/scrcpy/server.rs:142` - `should_lock_orientation_for_nvdec()`
- **用途**: 优化提示 (NVDEC 性能更好时锁定捕获方向)
- **非关键**: 检测失败不影响解码器选择,仅影响性能优化
- **未来考虑**: 改为尝试 NVDEC 初始化成功后再锁定方向

**Breaking Changes**:

- `DeviceProfile::build_options_string(gpu_type: GpuType)` → `build_options_string()` (移除参数)
- 现有 `device_profiles.toml` 可能需要重新生成 (profile 选择逻辑变化)

**相关提交**:

- Decoder cascade fallback: `src/decoder/auto.rs` (2026-01-23)
- Codec probe cascade testing: `src/scrcpy/codec_probe.rs` (2026-01-23)

**参考资料**:

- FFmpeg NVDEC 文档: https://trac.ffmpeg.org/wiki/HWAccelIntro#NVDEC
- FFmpeg VAAPI 文档: https://trac.ffmpeg.org/wiki/Hardware/VAAPI
- FFmpeg D3D11VA 文档: https://ffmpeg.org/ffmpeg-codecs.html#h264

---

## Pitfall #20: AMD GPU D3D11VA 硬件加速初始化成功但解码失败 (2026-01-27)

**问题描述**:

在 Windows 上使用 AMD GPU 时,D3D11VA 硬件加速器初始化成功(`D3D11VA device context created successfully`),但实际解码 H.264 数据时持续失败:

```
[h264 @ 0000019ECCA4E9C0] Failed setup for format d3d11: hwaccel initialisation returned error.
[h264 @ 0000019ECCA4E9C0] decode_slice_header error
[h264 @ 0000019ECCA4E9C0] no frame!
```

**根本原因**:

1. **FFmpeg 标志不兼容**: `AV_CODEC_FLAG2_FAST` 和 `FF_COMPLIANCE_EXPERIMENTAL` 在某些 AMD GPU 驱动上会导致 D3D11VA 初始化失败
2. **Profile 不匹配**: AMD D3D11VA 对 H.264 profile 有严格要求,需要 `AV_HWACCEL_FLAG_ALLOW_PROFILE_MISMATCH`
3. **错误诊断不足**: 原代码只输出通用错误码,无法定位具体问题(驱动版本、GPU 型号、兼容性)

**错误场景**:

```rust
// 旧代码 (不兼容 AMD GPU)
unsafe {
    (*ctx_ptr).flags2 |= ffmpeg::sys::AV_CODEC_FLAG2_FAST;  // AMD GPU 拒绝此标志
    (*ctx_ptr).strict_std_compliance = ffmpeg::sys::FF_COMPLIANCE_EXPERIMENTAL;  // 过于宽松
    // 未设置 hwaccel_flags,导致 profile 不匹配时直接失败
}
```

**修复方案**:

```rust
// AMD GPU 兼容修复 (src/decoder/d3d11va.rs)
unsafe {
    let ctx_ptr = context.as_mut_ptr();
    
    (*ctx_ptr).hw_device_ctx = ffmpeg::sys::av_buffer_ref(hw_device_ctx);
    (*ctx_ptr).get_format = Some(get_d3d11va_format);
    
    // AMD GPU compatibility: Use conservative flags to avoid decoder rejection
    (*ctx_ptr).flags |= ffmpeg::sys::AV_CODEC_FLAG_LOW_DELAY as i32;
    (*ctx_ptr).strict_std_compliance = ffmpeg::sys::FF_COMPLIANCE_NORMAL;  // 改为 NORMAL
    (*ctx_ptr).thread_count = 1;
    
    // AMD GPU workaround: Explicitly set hwaccel flags for better compatibility
    (*ctx_ptr).hwaccel_flags |= ffmpeg::sys::AV_HWACCEL_FLAG_ALLOW_PROFILE_MISMATCH;
}

// 硬件支持验证 (初始化后检查)
fn verify_hardware_support(&mut self) -> Result<()> {
    unsafe {
        let hw_config = ffmpeg::sys::avcodec_get_hw_config((*ctx_ptr).codec, 0);
        if hw_config.is_null() {
            return Err(VideoError::InitializationError(
                "D3D11VA: No hardware config found. GPU may not support D3D11VA.".to_string()
            ));
        }
        // 检查 device_type 是否匹配 AV_HWDEVICE_TYPE_D3D11VA
    }
    Ok(())
}

// 增强错误诊断
fn receive_frames(&mut self) -> Result<Vec<DecodedFrame>> {
    let mut consecutive_failures = 0;
    const MAX_CONSECUTIVE_FAILURES: u32 = 5;
    
    loop {
        match self.decoder.receive_frame(&mut hw_frame) {
            Err(e) => {
                consecutive_failures += 1;
                if consecutive_failures >= MAX_CONSECUTIVE_FAILURES {
                    return Err(VideoError::DecodingError(format!(
                        "D3D11VA: {} consecutive failures. GPU may be unsupported. Update drivers or disable hwdecode.",
                        consecutive_failures
                    )));
                }
            }
            // ...
        }
    }
}
```

**变更影响**:

1. ✅ **AMD GPU 兼容性提升**: 移除 `AV_CODEC_FLAG2_FAST`,使用 `FF_COMPLIANCE_NORMAL`
2. ✅ **Profile 容错**: `AV_HWACCEL_FLAG_ALLOW_PROFILE_MISMATCH` 允许 Baseline/Main/High profile 混用
3. ✅ **早期失败检测**: `verify_hardware_support()` 在初始化后立即验证硬件支持
4. ✅ **连续失败保护**: 5 次失败后自动降级,避免无限重试
5. ✅ **详细错误消息**: 提示用户更新驱动或禁用 `hwdecode`

**已知限制**:

- **旧版 AMD 驱动**: 2020 年前的驱动可能仍不支持 D3D11VA H.264 硬解,建议更新至最新版
- **集成显卡**: AMD APU (如 Ryzen 5 5600G) 的 Vega iGPU 部分型号不支持 D3D11VA,需降级软解
- **多 GPU 系统**: FFmpeg 默认选择主 GPU,若主 GPU 不支持 D3D11VA,需手动禁用 `hwdecode`

**测试验证**:

```bash
# Windows 测试 (AMD GPU)
cargo run --release 2>&1 | Select-String "D3D11VA|decoder"

# 预期输出 (成功)
# INFO  saide::decoder::d3d11va > Initializing D3D11VA hardware decoder
# INFO  saide::decoder::d3d11va > D3D11VA device context created successfully
# DEBUG saide::decoder::d3d11va > D3D11VA hardware support verified
# INFO  saide::decoder::auto     > ✅ Using D3D11VA hardware decoder

# 预期输出 (失败 → 自动降级)
# WARN  saide::decoder::auto     > D3D11VA unavailable, falling back to software decoder
# INFO  saide::decoder::auto     > Using software H.264 decoder
```

**回退方案** (若仍失败):

```toml
# config.toml
[scrcpy.video]
hwdecode = false  # 强制软件解码
```

**相关文件**:

- 修复实现: `src/decoder/d3d11va.rs` (2026-01-27)
- 降级逻辑: `src/decoder/auto.rs` (NVDEC → D3D11VA → Software)

**参考资料**:

- AMD D3D11VA 驱动兼容性列表: https://www.amd.com/en/support/kb/release-notes
- FFmpeg hwaccel flags: https://ffmpeg.org/doxygen/trunk/group__lavc__core.html#ga8e6f251dbe6d48cfe8dc6f6d36a61dc1

---
