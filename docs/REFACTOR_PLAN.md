# 主程序重构计划：集成内部音视频实现

## 背景

render_avsync 示例已完全验证内部 scrcpy 实现可正常工作（音视频同步流畅），现在需要将其集成到主程序，替换外部 scrcpy + V4L2 方案。

## 当前架构（待移除）

```
主程序 → 外部 scrcpy 进程 → V4L2 虚拟设备 → V4l2Player → 渲染
         ↓ 
      adb shell input（控制）
```

**问题**：
- 依赖外部 scrcpy 进程和 V4L2 驱动
- 音频需要单独处理
- 延迟较高（多次转换）
- 配置复杂（scrcpy + v4l2 双重配置）

## 目标架构（新方案）

```
主程序 → ScrcpyConnection → {VideoDecoder, AudioDecoder} → 渲染+播放
         ↓
      adb shell input（保留）
```

**优势**：
- 纯 Rust 实现，无外部依赖
- 音视频统一处理
- 延迟更低（直接解码）
- 配置简化

## 分步重构计划

### 第1步：创建 StreamPlayer 模块 ✅（已完成草稿）

**位置**：`src/app/ui/stream_player.rs`

**功能**：
- 封装 ScrcpyConnection + AutoDecoder + AudioPlayer
- 提供与 V4l2Player 兼容的 API
- 独立线程处理音视频流

**接口**：
```rust
impl StreamPlayer {
    pub fn new(cc: &CreationContext) -> Self;
    pub fn start(&mut self, serial: String);
    pub fn stop(&mut self);
    pub fn update(&mut self); // 每帧调用
    pub fn render(&mut self, ui: &mut Ui);
    pub fn video_rect(&self) -> Rect;
    pub fn video_dimensions(&self) -> (u32, u32);
    pub fn fps(&self) -> f32;
    pub fn stats(&self) -> &StreamStats;
}
```

### 第2步：更新 SAideApp 使用 StreamPlayer

**文件**：`src/app/ui/saide.rs`

**改动**：
```rust
// 替换
player: V4l2Player,           → player: StreamPlayer,
scrcpy: Option<Scrcpy>,       → // 移除（内置）

// 初始化
StreamPlayer::new(cc)          // 简化，无需 config

// 启动
player.start(device_id)        // 直接传 serial

// 移除所有 scrcpy 进程管理代码
```

### 第3步：清理配置相关代码

**文件**：
- `src/config/mod.rs`
- `src/config/scrcpy.rs`

**移除配置项**：
- `scrcpy.v4l2.*` 全部（device, orientation 等）
- `scrcpy.video.*` 部分（max_size, max_fps 保留）
- `scrcpy.server.*` 全部（path, log_level 等）
- 外部 scrcpy 相关的所有选项

**保留配置**：
```toml
[scrcpy]
[scrcpy.video]
max_size = 1920
max_fps = 60
video_bit_rate = 8_000_000

[scrcpy.audio]
enabled = true
audio_codec = "opus"
audio_source = "playback"  # output/playback/mic
```

### 第4步：清理 main.rs 启动逻辑

**文件**：`src/main.rs`

**移除**：
```rust
info!("V4L2 device: {}", config.scrcpy.v4l2.device); // 移除
```

**更新**：
```rust
info!("Video backend: {}", config.gpu.backend);
info!("Max video size: {}", config.scrcpy.video.max_size);
info!("Audio: {}", if config.scrcpy.audio.enabled { "enabled" } else { "disabled" });
```

### 第5步：移除 v4l2 模块（可选，暂时保留）

**文件**：`src/v4l2/mod.rs`

**策略**：暂时保留但标记为 deprecated，未来版本移除
```rust
#[deprecated(note = "Use StreamPlayer instead")]
pub mod v4l2;
```

### 第6步：移除 controller/scrcpy.rs 外部进程管理

**文件**：`src/controller/scrcpy.rs`

**移除整个文件**（外部 scrcpy 进程管理器）

**保留**：`src/controller/mod.rs` 中的 adb input 控制

### 第7步：更新 Toolbar UI

**文件**：`src/app/ui/toolbar.rs`

**移除按钮/选项**：
- V4L2 device selector（如果有）
- 外部 scrcpy 相关选项

**保留**：
- 设备选择
- 旋转控制
- 键鼠映射开关

### 第8步：清理 init.rs 初始化流程

**文件**：`src/app/init.rs`

**简化**：
- 移除 V4L2 设备检测
- 移除外部 scrcpy 进程启动
- 只保留设备检测和基础初始化

### 第9步：更新文档

**文件**：
- `README.md` - 更新安装说明（移除 V4L2 要求）
- `docs/ARCHITECTURE.md` - 更新架构图
- `config.toml` - 更新示例配置

**新文档**：
- `docs/MIGRATION.md` - 从旧版本迁移指南

## 技术细节

### 依赖更新

**Cargo.toml 移除**：
```toml
# v4l2 相关（可选）
# v4l = "..."
```

**保留**：
- ffmpeg-next（解码器）
- opus（音频）
- cpal（音频播放）
- wgpu/egui（渲染）

### 错误处理

StreamPlayer 需要处理的错误：
1. 设备未找到
2. 连接失败
3. 解码器初始化失败
4. 流中断

建议：使用 `PlayerState` 枚举表示状态，UI 根据状态显示提示

### 性能优化

1. **帧缓冲**：`FRAME_BUFFER_SIZE = 3`（参考 render_avsync）
2. **音频缓冲**：200ms prebuffer（已在 AudioPlayer 中实现）
3. **同步策略**：AVSync threshold = 20ms
4. **丢帧策略**：late frame 直接丢弃

### 向后兼容

**配置文件兼容**：
```rust
// 读取旧配置时自动忽略废弃字段
#[serde(default)]
#[serde(skip_serializing_if = "Option::is_none")]
v4l2: Option<V4l2Config>, // 标记为 deprecated
```

**用户提示**：
```
⚠️  检测到旧版配置文件，部分选项已废弃：
   - scrcpy.v4l2.* (不再需要 V4L2)
   - scrcpy.server.* (使用内置实现)
   
   将自动迁移到新配置格式。
```

## 测试计划

### 单元测试
- StreamPlayer 初始化
- 连接建立
- 帧接收和渲染

### 集成测试
1. 设备连接 → 视频显示
2. 音频播放
3. 键鼠控制
4. 设备断开重连

### 回归测试
- 所有原有功能正常工作
- 性能不低于旧版本
- 延迟 < 100ms

## 风险和缓解

| 风险 | 影响 | 缓解措施 |
|------|------|----------|
| 新实现有 bug | 高 | render_avsync 已充分测试，直接复用代码 |
| 配置迁移失败 | 中 | 提供配置检查和迁移工具 |
| 性能下降 | 中 | 对比测试，优化瓶颈 |
| 用户升级困难 | 低 | 详细迁移文档 + 自动迁移 |

## 时间估计

| 任务 | 时间 | 优先级 |
|------|------|--------|
| StreamPlayer 实现 | 2-3h | P0 |
| SAideApp 集成 | 1-2h | P0 |
| 配置清理 | 1h | P1 |
| UI 更新 | 1h | P1 |
| 文档更新 | 1h | P2 |
| 测试验证 | 2h | P0 |
| **总计** | **8-10h** | |

## 下一步行动

1. ✅ 完成 StreamPlayer 基础实现（参考 render_avsync.rs）
2. 🔲 在 SAideApp 中集成 StreamPlayer 并验证基本功能
3. 🔲 逐步移除 v4l2/scrcpy 依赖
4. 🔲 清理配置文件
5. 🔲 完整测试并发布

## 备注

- 保持 git 提交原子化，每步独立提交
- 重构过程中保持主分支可编译
- 如遇问题回滚到上一个稳定提交
- 参考 `examples/render_avsync.rs` 作为黄金实现
