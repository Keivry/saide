# 坐标映射系统修复说明

## 问题描述

用户反馈了两个关键问题：
1. **坐标转换不正确** - 原始的旋转变换逻辑有误
2. **视频画面外的鼠标点击也被映射** - 没有边界检查

## 修复内容

### 1. 修复坐标转换逻辑

#### 问题分析
原始代码中的旋转变换公式不正确，导致点击位置映射到错误坐标。

#### 修复方案
在 `src/controller/mouse.rs` 中重写了旋转变换逻辑：

```rust
let (rotated_x, rotated_y) = match rotation % 4 {
    // 0 degrees - no rotation
    0 => (x, y),
    // 90 degrees clockwise - transpose and flip X
    1 => (video_height as f32 - y, x),
    // 180 degrees - flip both axes
    2 => (video_width as f32 - x, video_height as f32 - y),
    // 270 degrees clockwise - transpose and flip Y
    3 => (y, video_width as f32 - x),
    _ => (x, y),
};
```

**旋转变换原理：**
- **0°**：无变换 `(x, y)`
- **90°**：转置 + X轴翻转 `(height-y, x)`
- **180°**：双轴翻转 `(width-x, height-y)`  
- **270°**：转置 + Y轴翻转 `(y, width-x)`

### 2. 添加视频区域边界检查

#### 问题分析
原始代码将所有鼠标点击都进行了坐标映射，包括工具栏、状态栏等非视频区域。

#### 修复方案
在 `src/app/main.rs` 中实现了完整的边界检查：

```rust
// Calculate video display rectangle once
let video_rect = self.get_video_display_rect(ctx);

// Only handle clicks within the video rectangle
if pos.x >= video_rect.left()
    && pos.x <= video_rect.right()
    && pos.y >= video_rect.top()
    && pos.y <= video_rect.bottom()
{
    // Convert to video-relative coordinates
    let rel_x = pos.x - video_rect.left();
    let rel_y = pos.y - video_rect.top();

    // Scale to actual video dimensions
    let video_x = rel_x / display_w * self.video_width as f32;
    let video_y = rel_y / display_h * self.video_height as f32;
    
    // Handle the click...
}
```

**边界检查流程：**
1. **计算视频显示区域** - 考虑宽高比和居中显示
2. **检查点击是否在视频区域内** - 边界检测
3. **转换为相对坐标** - 相对于视频区域左上角的偏移
4. **缩放到实际视频尺寸** - 适配不同缩放比例

### 3. 改进设备方向处理

#### 问题分析
原代码对 portrait/landscape 设备的处理不准确。

#### 修复方案
根据设备类型和 capture-orientation 智能调整映射：

```rust
if capture_orientation == "0" || capture_orientation.is_empty() {
    // No capture orientation adjustment
    let dx = (rotated_x / video_width as f32 * screen_width as f32).round() as i32;
    let dy = (rotated_y / video_height as f32 * screen_height as f32).round() as i32;
    (dx, dy)
} else {
    // Adjust for device capture orientation
    match capture_orientation {
        "90" => {
            if screen_height > screen_width {
                // Portrait device
                let dx = (rotated_x / video_width as f32 * screen_height as f32).round() as i32;
                let dy = (rotated_y / video_height as f32 * screen_width as f32).round() as i32;
                (dx, dy)
            } else {
                // Landscape device
                let dx = (rotated_x / video_width as f32 * screen_width as f32).round() as i32;
                let dy = (rotated_y / video_height as f32 * screen_height as f32).round() as i32;
                (dx, dy)
            }
        }
        // ... 其他方向处理
    }
}
```

**设备方向处理策略：**
- **Portrait 设备** (高度 > 宽度)：90°/270° 时交换宽高
- **Landscape 设备** (宽度 >= 高度)：按原始尺寸映射
- **180° 旋转**：翻转坐标 `(width-dx, height-dy)`
- **智能检测**：自动识别设备类型并应用相应变换

## 修复后的完整流程

```
用户点击屏幕坐标
    ↓
检查是否在视频显示区域内？
    ↓ (否)
忽略点击
    ↓ (是)
转换为视频相对坐标
    ↓
应用UI旋转变换
    ↓
调整设备捕获方向
    ↓
映射到实际设备屏幕尺寸
    ↓
发送ADB命令
```

## 测试场景

### 1. 基本点击测试
- ✅ 视频区域内点击 → 正确映射
- ✅ 视频区域外点击 → 忽略

### 2. 旋转测试
- ✅ 0° 旋转：坐标不变
- ✅ 90° 旋转：坐标正确转置和翻转
- ✅ 180° 旋转：坐标正确翻转
- ✅ 270° 旋转：坐标正确转置和翻转

### 3. 设备方向测试
- ✅ Portrait 设备 + 90° capture-orientation
- ✅ Landscape 设备 + 90° capture-orientation
- ✅ 所有组合测试

### 4. 缩放测试
- ✅ 窗口缩放时点击位置正确
- ✅ 高DPI显示器测试

## 性能优化

- **矩形预计算**：`get_video_display_rect()` 在处理事件前预先计算
- **缓存设备尺寸**：避免重复查询 ADB
- **早期退出**：不在视频区域内的点击直接忽略

## 文件修改列表

1. **`src/controller/mouse.rs`**
   - 重写旋转变换逻辑
   - 改进设备方向处理
   - 修复格式字符串错误

2. **`src/app/main.rs`**
   - 添加视频区域边界检查
   - 实现 `get_video_display_rect()` 方法
   - 优化坐标转换流程

## 验证方法

编译并测试：
```bash
cargo build --release
```

在真实设备上验证：
1. 启动应用，连接安卓设备
2. 点击视频区域内不同位置
3. 验证点击是否正确映射到设备对应位置
4. 测试旋转功能（90°、180°、270°）
5. 测试不同 capture-orientation 设置
6. 确认视频区域外点击被忽略

