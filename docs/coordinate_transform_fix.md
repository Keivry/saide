# 坐标转换问题修复方案

## 问题分析

### ADB Shell vs Scrcpy 控制通道的坐标系差异

| 方面 | ADB Shell | Scrcpy Control Channel |
|------|-----------|------------------------|
| **坐标系** | 设备逻辑坐标系（考虑旋转后） | 视频帧坐标系（固定） |
| **分辨率** | 设备物理分辨率 | 视频编码分辨率 |
| **旋转处理** | 客户端必须计算 | 服务端自动处理 |
| **screenSize** | 设备当前尺寸 | 视频当前尺寸 |

### 示例说明

**场景**：设备 1260x2800，横屏旋转（orientation=1），视频 1280x576

#### ADB Shell 期望
```
设备横屏：2800x1260
点击中心：(1400, 630)
```

#### Scrcpy Control Channel 期望
```
视频尺寸：1280x576（固定，不随设备旋转）
点击中心：(640, 288)
screenSize: 1280x576（发送到服务端）
```

服务端会自动应用 `videoToDeviceMatrix` 转换到设备坐标。

---

## 修复方案

### 方案 1：创建新的坐标转换函数（推荐）

在 `utils.rs` 中添加：

```rust
/// Transform egui position to video coordinates for scrcpy control channel
///
/// Scrcpy 控制通道期望：
/// - 坐标相对于视频分辨率（不是设备分辨率）
/// - 不需要考虑设备旋转（服务端会自动处理）
/// - screenSize = video resolution
pub fn screen_to_video_coords(
    pos: &egui::Pos2,
    video_rect: &egui::Rect,
    video_rotation: u32,
) -> Option<(u32, u32, u16, u16)> {
    // Step 1: Get relative coordinates in video display rect
    let rel_x = pos.x - video_rect.left();
    let rel_y = pos.y - video_rect.top();

    let video_width = video_rect.width();
    let video_height = video_rect.height();

    // Step 2: Inverse user rotation to get video original coordinates
    let (video_x, video_y, video_w, video_h) = match video_rotation % 4 {
        0 => (rel_x, rel_y, video_width, video_height),
        1 => (rel_y, video_width - rel_x, video_height, video_width),
        2 => (video_width - rel_x, video_height - rel_y, video_width, video_height),
        3 => (video_height - rel_y, rel_x, video_height, video_width),
        _ => return None,
    };

    // Return: (x, y, screenWidth, screenHeight)
    // screenWidth/Height = 视频原始尺寸
    Some((
        video_x as u32,
        video_y as u32,
        video_w as u16,
        video_h as u16,
    ))
}
```

### 方案 2：修改 MouseMapper 和 KeyboardMapper

在 `saide.rs` 的鼠标/键盘事件处理中：

```rust
// 旧代码（ADB shell）
let device_coords = screen_to_device_coords(pos, &self.coodinates_transform_params())?;
self.mouse_mapper.handle_button_event(button, pressed, device_coords.0, device_coords.1)?;

// 新代码（Scrcpy）
let (x, y, width, height) = screen_to_video_coords(
    pos,
    &self.player.video_rect(),
    self.player.rotation(),
)?;

// ControlSender 需要更新 screen_size
if let Some(sender) = &self.control_sender {
    sender.update_screen_size(width, height);
    self.mouse_mapper.handle_button_event(button, pressed, x, y)?;
}
```

---

## 实施步骤

1. **在 utils.rs 添加 `screen_to_video_coords()`**
   - 简化版坐标转换（只处理用户旋转）
   - 返回 (x, y, screenWidth, screenHeight)

2. **修改 saide.rs 事件处理**
   - 鼠标事件：使用 `screen_to_video_coords()`
   - 键盘自定义映射：继续使用 `screen_to_device_coords()`（因为 config.toml 存的是设备坐标）

3. **动态更新 ControlSender 的 screen_size**
   - 在每次鼠标/键盘事件前检查视频尺寸是否变化
   - 如果变化则调用 `control_sender.update_screen_size()`

4. **测试验证**
   - 横屏/竖屏旋转
   - 点击精度
   - 拖拽轨迹

---

## 注意事项

### ControlSender 的 screen_size 维护

**问题**：视频分辨率可能动态变化（设备旋转、分辨率调整）

**解决**：
- 在 `PlayerEvent::ResolutionChanged` 时更新 ControlSender
- 或在每次输入事件时从 video_rect 实时计算

### 兼容性

**自定义映射**：config.toml 中的坐标是设备坐标，需要转换

```rust
// 自定义映射：config 坐标 → scrcpy 坐标
fn device_to_video_coords(
    device_x: u32,
    device_y: u32,
    device_size: (u32, u32),
    video_size: (u16, u16),
    device_orientation: u32,
) -> (u32, u32) {
    // 1. 反向旋转（设备坐标 → portrait）
    // 2. 缩放（设备分辨率 → 视频分辨率）
    // 3. 应用 capture_orientation（如果需要）
}
```

---

## 参考

- scrcpy 源码：`server/.../PositionMapper.java`
- scrcpy 源码：`app/src/screen.c:sc_screen_convert_drawable_to_frame_coords()`
- 当前实现：`src/app/utils.rs:screen_to_device_coords()`
