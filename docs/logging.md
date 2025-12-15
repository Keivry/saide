# 日志配置指南

## 日志级别

SAide 使用 `tracing` 库进行日志记录，支持以下级别：

### TRACE（跟踪）
**高频事件**，用于详细的性能分析和问题诊断：
- 鼠标移动事件（每次移动）
- 键盘按键事件（每次按键）
- 坐标转换详情（每次输入）
- 音频解码（每 20ms）
- 视频解码（每帧，60fps）
- 纹理上传（每帧）

### DEBUG（调试）
**调试信息**，用于开发和问题排查：
- 连接建立/断开
- 初始化过程
- 设备旋转
- 分辨率变化
- 状态转换
- 配置加载

### INFO（信息）
**关键流程**，用于了解程序运行状态：
- 程序启动/关闭
- 编解码器选择
- 窗口调整
- 设备连接
- Profile 加载

### WARN（警告）
**警告信息**，需要注意但不影响运行：
- 功能降级
- 兼容性问题
- 性能警告

### ERROR（错误）
**错误信息**，影响功能正常使用：
- 连接失败
- 解码错误
- 配置错误

---

## 使用方式

### 1. 基础用法

```bash
# 只显示 INFO 及以上级别（推荐日常使用）
RUST_LOG=info cargo run

# 显示 DEBUG 及以上级别（开发调试）
RUST_LOG=debug cargo run

# 显示所有日志（性能分析）
RUST_LOG=trace cargo run
```

### 2. 过滤第三方库日志

第三方库（wgpu、eframe、winit）会产生大量 DEBUG 日志，可以单独过滤：

```bash
# 只显示 SAide 的 debug 日志，第三方库只显示 warn
RUST_LOG=saide=debug,wgpu=warn,eframe=warn,winit=warn cargo run

# 更精细的控制
RUST_LOG=saide=trace,wgpu_hal=error,eframe=info cargo run

# 只看特定模块的 trace
RUST_LOG=saide::decoder=trace,saide=info cargo run
```

### 3. 常用配置

**日常使用**（清晰简洁）：
```bash
RUST_LOG=saide=info cargo run
```

**开发调试**（包含状态变化）：
```bash
RUST_LOG=saide=debug,wgpu=warn,eframe=warn cargo run
```

**性能分析**（完整帧信息）：
```bash
RUST_LOG=saide=trace cargo run 2>&1 | grep "saide::"
```

**只看音视频**：
```bash
RUST_LOG=saide::decoder=trace,saide::sync=debug cargo run
```

---

## 常见问题

### Q: 退出时出现大量 "Asking to exit event loop"

这是 eframe 库的正常行为，可以过滤：

```bash
RUST_LOG=saide=debug,eframe::native::run=info cargo run
```

### Q: 退出时出现 CUDA 错误

这是因为窗口关闭时 CUDA 上下文已销毁，但解码线程还在运行。这些错误已经降级到 `trace` 级别，使用 `debug` 或 `info` 级别不会看到。

### Q: 日志输出太多，看不清关键信息

使用管道过滤：

```bash
# 只看错误和警告
RUST_LOG=debug cargo run 2>&1 | grep -E "WARN|ERROR"

# 排除某些模块
RUST_LOG=trace cargo run 2>&1 | grep -v "wgpu_hal"

# 只看 SAide 模块
RUST_LOG=trace cargo run 2>&1 | grep "saide::"
```

---

## 开发建议

### 日志级别选择原则

1. **TRACE**：频率 > 10次/秒 的事件
2. **DEBUG**：状态变化、初始化、连接事件
3. **INFO**：用户关心的关键流程
4. **WARN**：可恢复的异常情况
5. **ERROR**：不可恢复的错误

### 添加新日志

```rust
use tracing::{trace, debug, info, warn, error};

// 高频事件（每帧/每次输入）
trace!("Frame decoded: {}x{}", width, height);

// 状态变化
debug!("State transition: {:?} -> {:?}", old, new);

// 关键流程
info!("Connected to device: {}", device_id);

// 警告
warn!("Using fallback decoder due to: {}", reason);

// 错误
error!("Failed to initialize: {}", err);
```

---

## 环境变量配置

可以在 `~/.bashrc` 或 `~/.zshrc` 中设置默认日志级别：

```bash
# 添加到 shell 配置文件
export RUST_LOG=saide=info,wgpu=warn,eframe=warn
```

或者创建一个启动脚本 `run.sh`：

```bash
#!/bin/bash
export RUST_LOG=saide=debug,wgpu=warn,eframe=warn,winit=warn
cargo run
```
