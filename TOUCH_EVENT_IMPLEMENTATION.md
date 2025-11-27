# Touch Event Implementation - 触摸事件实现

## Problem Statement / 问题描述

The original mouse drag implementation used `input swipe` commands for all touch interactions. This caused Android to interpret drag operations as tap/click events, resulting in unwanted click actions when users tried to drag.

原始的鼠标拖动实现对所有触摸交互都使用 `input swipe` 命令。这导致安卓将拖动操作解释为点击事件，当用户尝试拖动时会产生不需要的点击动作。

## Solution / 解决方案

Implemented a proper touch event sequence using Android's motionevent commands to distinguish between different touch interactions:

实现了适当的触摸事件序列，使用安卓的 motionevent 命令来区分不同的触摸交互：

### 1. Touch Event Types / 触摸事件类型

Added three new ADB action types in `src/config/mapping.rs:72-86`:

在 `src/config/mapping.rs:72-86` 中添加了三种新的 ADB 动作类型：

```rust
/// Touch down event (start of drag)
TouchDown { x: u32, y: u32 },
/// Touch move event (during drag)
TouchMove { x: u32, y: u32 },
/// Touch up event (end of drag)
TouchUp { x: u32, y: u32 },
```

### 2. Command Generation / 命令生成

Modified `src/controller/adb.rs:177-185` to generate proper motionevent commands:

修改 `src/controller/adb.rs:177-185` 以生成正确的 motionevent 命令：

```rust
AdbAction::TouchDown { x, y } => {
    format!("input motionevent DOWN {} {}\n", x, y)
}
AdbAction::TouchMove { x, y } => {
    format!("input motionevent MOVE {} {}\n", x, y)
}
AdbAction::TouchUp { x, y } => {
    format!("input motionevent UP {} {}\n", x, y)
}
```

### 3. Mouse State Machine / 鼠标状态机

Implemented comprehensive state machine in `src/controller/mouse.rs:12-29`:

在 `src/controller/mouse.rs:12-29` 中实现了完整的状态机：

```rust
enum MouseState {
    Idle,
    Pressed { x: u32, y: u32, time: Instant },
    Dragging { start_x: u32, start_y: u32, current_x: u32, current_y: u32, last_update: Instant },
    LongPressing { x: u32, y: u32 },
}
```

### 4. State Transitions / 状态转换

| From | Trigger | To | Action |
|------|---------|----|----|
| Pressed | Time ≥ 200ms | LongPressing | Long press detected (no ADB event) |
| LongPressing | Mouse move | Dragging | Start drag from long press position |
| Dragging | Mouse release | Idle | Complete drag sequence |
| LongPressing | Mouse release | Idle | Complete long press sequence |

### 4. Touch Event Sequences / 触摸事件序列

#### Tap / 点击
```
Mouse Press → TouchDown
Mouse Release → TouchUp
```

#### Long Press / 长按
```
Mouse Press → TouchDown
Wait 200ms (monitored by update())
Long press detected → State transition to LongPressing (no ADB event sent)
Mouse Release → TouchUp
```

#### Drag / 拖动
```
Mouse Press → TouchDown
Mouse Move → (distance >= 2px) → Transition to Dragging
Drag Updates → TouchMove (every 50ms if position changed)
Mouse Release → TouchUp
```

## Key Implementation Details / 关键实现细节

### Drag Threshold / 拖动阈值
- **Value**: 2 pixels / **值**: 2 像素
- **Purpose**: Distinguish between tap and drag / **目的**: 区分点击和拖动
- **Location**: `src/controller/mouse.rs:39` / **位置**: `src/controller/mouse.rs:39`

### Long Press Duration / 长按持续时间
- **Value**: 200ms / **值**: 200 毫秒
- **Purpose**: Trigger long press when mouse button held / **目的**: 鼠标按键按住时触发长按
- **Location**: `src/controller/mouse.rs:41` / **位置**: `src/controller/mouse.rs:41`

### Drag Update Interval / 拖动更新间隔
- **Value**: 50ms / **值**: 50 毫秒
- **Purpose**: Balance between smoothness and performance / **目的**: 平衡流畅性和性能
- **Location**: `src/controller/mouse.rs:43` / **位置**: `src/controller/mouse.rs:43`

### Active Polling for Long Press / 长按主动轮询
Since egui doesn't send mouse move events when the mouse is stationary, implemented `update()` method that runs every frame to check for long press timeout:

由于 egui 在鼠标静止时不会发送鼠标移动事件，实现了 `update()` 方法，每帧检查长按超时：

```rust
pub fn update(&self) -> Result<()> {
    // Check MouseState::Pressed for long press timeout
    // If exceeded, send long press event and transition to LongPressing
    // Update MouseState::Dragging to send TouchMove events
}
```

## Modified Files / 修改的文件

1. **src/config/mapping.rs**
   - Added `TouchDown`, `TouchMove`, `TouchUp` enum variants

2. **src/controller/adb.rs**
   - Added command generation for motionevent DOWN/MOVE/UP

3. **src/controller/mouse.rs**
   - Implemented complete state machine
   - Added `update()` method for long press detection
   - Modified all touch interactions to use proper event sequences
   - Changed drag updates from `Swipe` to `TouchMove`

## Expected Behavior / 预期行为

### Before / 修改前
- **Drag**: Swipe commands → Android interprets as tap → Unwanted click
- **Long Press**: TouchDown + Swipe(x,y→x,y,500ms) + TouchUp → Android interprets as tap → Unwanted click

### After / 修改后
- **Drag**: TouchDown → TouchMove (repeated) → TouchUp → Proper drag gesture
- **Long Press**: TouchDown → (hold 200ms) → State change only → TouchUp → Android auto-detects long press

## Key Fix: Long Press Event Handling / 关键修复：长按事件处理

**Problem**: Initially, long press triggered a Swipe event with same start/end coordinates (x,y→x,y, duration=500ms). Android interpreted this sequence as: TouchDown → Short Move → TouchUp = **Tap**, causing long press to fail.

**解决方案**: 最初，长按触发一个起点/终点坐标相同的 Swipe 事件 (x,y→x,y, 持续=500ms)。安卓将此序列解释为：TouchDown → 短移动 → TouchUp = **点击**，导致长按失败。

**Solution**: Removed Swipe event from long press handling. Instead, only change state to LongPressing. The existing TouchDown event remains held, allowing Android to naturally detect the sustained touch and trigger long press automatically.

**解决方案**: 从长按处理中移除 Swipe 事件。改为仅将状态改为 LongPressing。保持现有的 TouchDown 事件，让安卓自然检测持续触摸并自动触发长按。

**Code Location**: `src/controller/mouse.rs:65-75`

```rust
if elapsed >= LONG_PRESS_DURATION_MS {
    // Long press triggered - don't send any ADB event
    // Android will detect the sustained touch and trigger long press automatically
    // The TouchDown is already being held, so Android knows it's a long press
    debug!("Long press triggered at ({}, {}) [from update]", x, y);

    *state = MouseState::LongPressing { x, y };
}
```

## Testing Recommendations / 测试建议

1. **Tap Test**: Click on app icons - should open without triggering drag
2. **Drag Test**: Drag app icons across screen - should move without clicking
3. **Long Press Test**: Hold mouse button on icon for 200ms - should trigger context menu
4. **Device Rotation Test**: Rotate device and verify coordinates remain accurate

## Notes / 注意事项

- Long press still uses `Swipe` command (appropriate for sustained touch)
- Mouse wheel events continue to use `Swipe` (appropriate for scroll gestures)
- All other interactions use proper touch event sequence
- Touch events provide lower-level control over Android's touch subsystem
