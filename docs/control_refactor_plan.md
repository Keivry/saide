# 输入控制重构方案

## 目标
将鼠标和键盘输入从 ADB shell 命令改为使用 scrcpy 控制通道，实现：
- ✅ 更低延迟（无需 shell 解析）
- ✅ 更精确的事件时序
- ✅ 与 scrcpy 官方实现完全兼容

## 架构变更

### 当前架构（问题）
```
SAideApp
  ├── KeyboardMapper  ──> AdbShell ──> adb shell input keyevent
  ├── MouseMapper     ──> AdbShell ──> adb shell input motionevent
  └── StreamPlayer
        └── stream_worker (async)
              └── ScrcpyConnection { control_stream } ❌ 被困在线程内
```

### 新架构
```
SAideApp
  ├── ScrcpyConnection { control_stream }  ✅ 提升到 App 层
  ├── ControlSender { control_stream clone }  ✅ 共享控制通道
  ├── KeyboardMapper ──> ControlSender ──> control_stream.write()
  ├── MouseMapper    ──> ControlSender ──> control_stream.write()
  └── StreamPlayer { video_stream, audio_stream }  ✅ 只管理播放
```

## 实施步骤

### 阶段 1：创建 ControlSender 模块
**文件**: `src/controller/control_sender.rs`

功能：
- 封装 `TcpStream` 的克隆引用
- 提供类型安全的控制消息发送方法
- 处理序列化和错误

接口设计：
```rust
pub struct ControlSender {
    stream: Arc<Mutex<TcpStream>>,
    screen_width: Arc<Mutex<u16>>,
    screen_height: Arc<Mutex<u16>>,
}

impl ControlSender {
    pub fn new(stream: TcpStream, width: u16, height: u16) -> Self;
    
    // Helper methods using ControlMessage
    pub fn send_touch_down(&self, x: u32, y: u32) -> Result<()>;
    pub fn send_touch_up(&self, x: u32, y: u32) -> Result<()>;
    pub fn send_touch_move(&self, x: u32, y: u32) -> Result<()>;
    pub fn send_key_down(&self, keycode: u32, metastate: u32) -> Result<()>;
    pub fn send_key_up(&self, keycode: u32, metastate: u32) -> Result<()>;
    pub fn send_scroll(&self, x: u32, y: u32, h: f32, v: f32) -> Result<()>;
    pub fn send_text(&self, text: &str) -> Result<()>;
    pub fn send_back(&self) -> Result<()>;
    
    pub fn update_screen_size(&self, width: u16, height: u16);
}
```

### 阶段 2：重构 KeyboardMapper
**文件**: `src/controller/keyboard.rs`

变更：
- 移除 `AdbShell` 依赖
- 添加 `ControlSender` 字段
- 实现 egui Key → Android Keycode 转换
- 所有输入通过 `ControlMessage` 发送

修改点：
```rust
pub struct KeyboardMapper {
    config: Arc<Mappings>,
    sender: ControlSender,  // ← 替换 adb_shell
    // ... 其他字段保持不变
}

impl KeyboardMapper {
    pub fn new(config: Arc<Mappings>, sender: ControlSender) -> Result<Self>;
    
    // 移除 connect() 方法（不再需要）
    
    pub fn handle_standard_key_event(&self, key: &Key) -> Result<bool> {
        if let Some(&keycode) = EGUI_TO_ANDROID_KEY.get(key) {
            self.sender.send_key_down(keycode as u32, 0)?;
            self.sender.send_key_up(keycode as u32, 0)?;
            return Ok(true);
        }
        Ok(false)
    }
    
    // 类似地修改 handle_text_input_event, handle_keycombo_event 等
}
```

### 阶段 3：重构 MouseMapper
**文件**: `src/controller/mouse.rs`

变更：
- 移除 `AdbShell` 依赖
- 添加 `ControlSender` 字段
- 直接使用 ControlMessage 发送触摸事件

修改点：
```rust
pub struct MouseMapper {
    sender: ControlSender,  // ← 替换 adb_shell
    left_button_state: Mutex<MouseState>,
}

impl MouseMapper {
    pub fn new(sender: ControlSender) -> Result<Self>;
    
    fn handle_left_button_press(&self, x: u32, y: u32) -> Result<()> {
        self.sender.send_touch_down(x, y)?;  // ← 直接调用
        // ...
    }
    
    // 类似地修改其他方法
}
```

### 阶段 4：修改 StreamPlayer
**文件**: `src/app/ui/stream_player.rs`

变更：
- 修改 `start()` 接受已建立的 streams
- 移除内部 `ScrcpyConnection::connect()` 调用
- 简化为纯粹的播放器

修改点：
```rust
impl StreamPlayer {
    pub fn start(
        &mut self,
        video_stream: TcpStream,
        audio_stream: Option<TcpStream>,
        video_resolution: (u32, u32),
        config: ScrcpyConfig,
    ) {
        // 启动 stream_worker，但不再建立连接
        self.stream_thread = Some(thread::spawn(move || {
            stream_worker_with_streams(
                video_stream,
                audio_stream,
                video_resolution,
                config,
                event_tx,
            )
        }));
    }
}
```

### 阶段 5：修改 SAideApp 初始化
**文件**: `src/app/ui/saide.rs`

变更：
- 在 init 阶段建立 `ScrcpyConnection`
- 提取 `control_stream` 并创建 `ControlSender`
- 传递 streams 给 `StreamPlayer`
- 使用 `ControlSender` 创建 mappers

修改点：
```rust
pub struct SAideApp {
    connection: Option<ScrcpyConnection>,  // ← 新增
    control_sender: Option<ControlSender>, // ← 新增
    keyboard_mapper: Option<KeyboardMapper>,
    mouse_mapper: Option<MouseMapper>,
    player: StreamPlayer,
    // ...
}

// 在初始化完成后：
fn on_init_ready(&mut self) {
    // 1. 建立连接
    let mut conn = ScrcpyConnection::connect(...).await?;
    
    // 2. 提取 streams
    let video_stream = conn.video_stream.take()?;
    let audio_stream = conn.audio_stream.take();
    let control_stream = conn.control_stream.take()?;
    let video_resolution = conn.video_resolution.unwrap();
    
    // 3. 创建 ControlSender
    let sender = ControlSender::new(
        control_stream,
        video_resolution.0 as u16,
        video_resolution.1 as u16,
    );
    
    // 4. 创建 mappers
    self.keyboard_mapper = Some(KeyboardMapper::new(config, sender.clone())?);
    self.mouse_mapper = Some(MouseMapper::new(sender.clone())?);
    
    // 5. 启动播放器
    self.player.start(video_stream, audio_stream, video_resolution, config);
    
    // 6. 保存引用
    self.connection = Some(conn);
    self.control_sender = Some(sender);
}
```

## 优势对比

| 方面 | ADB Shell (旧) | Control Channel (新) |
|------|----------------|---------------------|
| **延迟** | 50-100ms（需解析命令） | 5-10ms（二进制直发） |
| **精度** | 整数坐标，有舍入误差 | 浮点压力，完整 metastate |
| **兼容性** | 依赖 shell 版本 | scrcpy 协议保证 |
| **依赖** | 需要 adb 可执行文件 | 无外部依赖 |
| **错误处理** | 无返回值，静默失败 | TCP 流错误即时检测 |

## 待办事项

- [ ] 实现 ControlSender 模块
- [ ] 重构 KeyboardMapper 使用 ControlSender
- [ ] 重构 MouseMapper 使用 ControlSender
- [ ] 修改 StreamPlayer 接口
- [ ] 修改 SAideApp 初始化流程
- [ ] 更新 AdbAction 自定义映射逻辑
- [ ] 测试所有输入场景
- [ ] 更新 TODO.md
- [ ] 删除废弃的 AdbShell 代码（可选，保留作为备用）

## 兼容性注意

保留 `AdbShell` 的静态方法供其他功能使用：
- `get_physical_screen_size()` - 用于初始化
- `get_screen_orientation()` - 用于旋转检测
- `get_ime_state()` - 用于输入法状态
- `get_device_id()` - 用于设备识别

这些方法不涉及实时输入，可以继续使用单次 adb 命令。
