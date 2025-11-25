# 坐标映射系统改进说明

## 问题描述

原始代码中的坐标映射存在以下问题：
1. 使用硬编码的屏幕尺寸（1080x1920）
2. 没有考虑设备实际屏幕尺寸
3. 没有处理视频旋转
4. 没有处理 scrcpy 的 capture-orientation 参数

## 解决方案

### 1. 获取真实设备屏幕尺寸

在 `src/controller/adb.rs` 中：

```rust
/// Get Android device screen size using separate adb command
pub fn get_screen_size(&self) -> Result<(u32, u32)> {
    // Use separate adb command to get screen size, not through shell session
    let output = Command::new("adb")
        .args(&["shell", "wm size"])
        .output()
        .context("Failed to execute adb shell wm size command")?;

    let output_str = String::from_utf8_lossy(&output.stdout);
    info!("wm size output: {}", output_str.trim());

    // Parse output like "Physical size: 1080x2340" or "Override size: 1080x2340"
    if let Some(line) = output_str.lines().find(|line| {
        line.contains("Physical size:") || line.contains("Override size:")
    }) {
        if let Some(size_part) = line.split(':').nth(1) {
            let size_str = size_part.trim();
            let parts: Vec<&str> = size_str.split('x').collect();
            if parts.len() == 2 {
                let width = parts[0].trim().parse::<u32>().unwrap_or(1080);
                let height = parts[1].trim().parse::<u32>().unwrap_or(1920);
                return Ok((width, height));
            }
        }
    }

    warn!("Failed to parse screen size from output, using default (1080x1920)");
    Ok((1080, 1920))
}
```

**关键特性：**
- 使用 `adb shell wm size` 命令获取真实设备尺寸
- 解析 "Physical size: 1080x2340" 或 "Override size: 1080x2340" 格式
- 支持回退到默认值（1080x1920）如果解析失败

### 2. 缓存屏幕尺寸

在 `AdbShell` 结构体中添加了缓存机制：

```rust
pub struct AdbShell {
    // ...
    /// Android device screen dimensions
    screen_size: Arc<Mutex<(u32, u32)>>,
}

impl AdbShell {
    pub fn new() -> Self {
        Self {
            // ...
            screen_size: Arc::new(Mutex::new((1080, 1920))), // Default size
        }
    }
    
    /// Get the cached screen size
    pub fn get_cached_screen_size(&self) -> (u32, u32) {
        let screen_size_lock = self.screen_size.lock().unwrap();
        *screen_size_lock
    }
}
```

在连接时自动获取并缓存屏幕尺寸：

```rust
pub fn connect(&self) -> Result<()> {
    // ...
    // Get device screen size
    if let Ok(size) = self.get_screen_size() {
        let mut screen_size_lock = self.screen_size.lock().unwrap();
        *screen_size_lock = size;
        info!("Device screen size: {}x{}", size.0, size.1);
    }
    // ...
}
```

### 3. 支持视频旋转

修改了 `MouseMapper` 的方法签名，添加了 `rotation` 参数：

```rust
pub fn handle_button_event(
    &self,
    button: &str,
    pressed: bool,
    x: f32,
    y: f32,
    video_width: u32,
    video_height: u32,
    rotation: u32,  // 新增参数
    capture_orientation: &str,  // 新增参数
) -> Result<()>
```

实现了旋转变换逻辑：

```rust
// Apply rotation transform to coordinates
let (transformed_x, transformed_y) = match rotation % 4 {
    // 0 degrees - no rotation
    0 => (x, y),
    // 90 degrees clockwise
    1 => (video_height as f32 - y, x),
    // 180 degrees
    2 => (video_width as f32 - x, video_height as f32 - y),
    // 270 degrees
    3 => (y, video_width as f32 - x),
    _ => (x, y),
};
```

### 4. 处理 scrcpy capture-orientation

根据 `capture_orientation` 参数调整坐标映射：

```rust
let (device_x, device_y) = match capture_orientation {
    "90" => {
        // Device is rotated 90 degrees
        let android_x = (transformed_x / video_width as f32 * screen_height as f32).round() as i32;
        let android_y = (transformed_y / video_height as f32 * screen_width as f32).round() as i32;
        (android_x, android_y)
    }
    "180" => {
        // Device is rotated 180 degrees
        let android_x = (transformed_x / video_width as f32 * screen_width as f32).round() as i32;
        let android_y = (transformed_y / video_height as f32 * screen_height as f32).round() as i32;
        (screen_width as i32 - android_x, screen_height as i32 - android_y)
    }
    "270" => {
        // Device is rotated 270 degrees
        let android_x = (transformed_x / video_width as f32 * screen_height as f32).round() as i32;
        let android_y = (transformed_y / video_height as f32 * screen_width as f32).round() as i32;
        (screen_height as i32 - android_x, screen_width as i32 - android_y)
    }
    _ => {
        // No rotation or unknown
        let android_x = (transformed_x / video_width as f32 * screen_width as f32).round() as i32;
        let android_y = (transformed_y / video_height as f32 * screen_height as f32).round() as i32;
        (android_x, android_y)
    }
};
```

### 5. 集成到主应用

在 `src/app/main.rs` 中的 `process_input_events` 方法中传递旋转和capture-orientation参数：

```rust
if let Err(e) = mouse_mapper.handle_button_event(
    button_str,
    *pressed,
    pos.x,
    pos.y,
    self.video_width,
    self.video_height,
    self.rotation,  // 传递应用旋转角度
    &self.config.scrcpy.v4l2.capture_orientation,  // 传递capture-orientation
) {
    error!("Failed to handle mouse event: {}", e);
}
```

## 坐标转换流程

完整的坐标转换流程：

```
1. 原始鼠标坐标 (video坐标)
   ↓
2. 应用UI旋转变换
   - 0°: (x, y)
   - 90°: (height-y, x)
   - 180°: (width-x, height-y)
   - 270°: (y, width-x)
   ↓
3. 考虑capture-orientation
   - 90/270: 交换width/height
   - 180: 翻转坐标
   ↓
4. 映射到设备屏幕尺寸
   device_x = transformed_x / video_width * screen_width
   device_y = transformed_y / video_height * screen_height
   ↓
5. 发送到ADB设备
```

## 文件修改列表

- `src/controller/adb.rs` - 添加屏幕尺寸获取和缓存
- `src/controller/mouse.rs` - 实现旋转和capture-orientation支持
- `src/app/main.rs` - 传递旋转参数到映射器

## 测试建议

1. 测试不同设备屏幕尺寸（手机、平板）
2. 测试不同capture-orientation设置（0、90、180、270）
3. 测试应用旋转功能
4. 测试边界坐标（0,0）、（width-1, height-1）

