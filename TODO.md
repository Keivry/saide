# SAide 任务清单

> **最后更新**: 2026-01-15  
> **当前状态**: ✅ Phase 1 延迟优化完成 (infrastructure + integration)

---

## P0 - 崩溃风险（必须立即修复）

### 错误处理 & Panic 风险

- [ ] **[panic]** `src/constant.rs:11`: `ProjectDirs::from(..).expect(..)` 在非标准环境（Docker/沙盒）会直接 panic  
  **解法**: 改为返回 `Result<PathBuf>`，在 `main.rs` 提供降级策略（临时目录 `/tmp/saide`）
  
- [ ] **[panic]** `src/config/mod.rs:121`: `path.to_str().unwrap()` 在非 UTF-8 路径（Windows 特殊字符）会 panic  
  **解法**: 使用 `to_string_lossy()` 或返回 `ConfigError::InvalidPath`

- [ ] **[panic]** `src/scrcpy/connection.rs:190-192`: `try_into().unwrap()` 无错误上下文  
  **解法**: 改用 `expect("Failed to parse video codec metadata: width/height")`

- [ ] **[panic]** `src/saide/ui/saide.rs:454,622,685,702,799,831,868`: 多处 `keyboard_mapper.unwrap()` 和 `mouse_mapper.unwrap()`  
  **解法**: 使用 `if let Some(mapper) = self.keyboard_mapper.as_mut()` 模式避免 panic

- [ ] **[panic]** `src/saide/coords.rs:152,194,275,326`: `unreachable!()` 在取模运算后（理论上可达，因为整数溢出）  
  **解法**: 改用 `debug_assert!` + 返回 `Result<MappingPos, CoordError>`

- [ ] **[panic]** `src/decoder/audio/player.rs:95`: cpal 音频回调中包含潜在 panic 点  
  **解法**: 回调函数中使用 `if let Ok(...) = ...` 模式，记录错误计数而非 panic

### 错误链丢失

- [ ] **[error-source]** `src/error.rs:18-46`: `IoError` 未实现 `std::error::Error::source()`，丢失错误链  
  **解法**: 将 `source_kind: io::ErrorKind` 改为存储 `source: Box<io::Error>`，实现 `source()` 方法

### 资源管理

- [ ] **[blocking-io]** `src/scrcpy/connection.rs:86`: `connect` 签名为 `async` 但内部使用阻塞 I/O（`TcpListener::accept`）  
  **解法**: 统一为同步 API（移除 `async`）或完全改用 `tokio::net::TcpListener`

- [ ] **[validation]** ADB 命令路径未验证：多处 `Command::new("adb")` 假设 ADB 在 PATH 中  
  **解法**: 在 `AdbShell::new()` 验证 `adb` 可执行性，缓存路径或提供 `config.adb_path` 覆盖

---

## P1 - 严重问题（影响稳定性或用户体验）

### 架构设计

- [ ] **[god-object]** `src/saide/ui/saide.rs:58-138`: `SAideApp` 包含 40+ 字段，违反单一职责原则  
  **解法**: 拆分为 `AppState`（连接/映射器）、`UIState`（工具栏/指示器/播放器）、`ConfigState`

- [ ] **[coupling]** `src/saide/init.rs`: 混合连接建立、设备监控、输入映射初始化三种职责  
  **解法**: 分离为 `ConnectionService::new()`, `DeviceMonitor::new()`, `InputManager::new()`

- [ ] **[coupling]** `src/saide/ui/player.rs:453-728`: `stream_worker` 275 行单体函数，混合解码器初始化、音频线程、视频循环  
  **解法**: 拆分为 `DecoderManager::init()`, `AudioThread::spawn()`, `VideoLoop::run()`

- [ ] **[abstraction]** `src/scrcpy/connection.rs:46-76`: `ScrcpyConnection` 公开原始 `TcpStream`  
  **解法**: 字段改为私有，仅暴露 `read_video_packet()`, `send_control()` 等方法

- [ ] **[dependency]** `src/error.rs:11-15`: 错误模块依赖 `decoder` 模块（应反向依赖）  
  **解法**: 将 `VideoError`, `AudioError` 移到独立 `decoder/error.rs`，错误模块仅定义 `SAideError`

### 配置与状态管理

- [ ] **[atomic-write]** `ConfigManager::save()` 缺少原子写机制（当前直接覆盖文件）  
  **解法**: 写到临时文件 `.config.toml.tmp` 再 `fs::rename()`（符合 POSIX 原子性）

- [ ] **[hardcoded]** `src/main.rs:18-19`: 窗口默认尺寸硬编码 `1280×720`，忽略 DPI 和屏幕尺寸  
  **解法**: 从配置读取或基于主屏幕尺寸动态计算（如 `80%` 高度）

- [ ] **[hardcoded]** `src/saide/init.rs:122`: `capture_orientation` 硬编码为 `Some(0)`  
  **解法**: 在 `SAideConfig` 新增 `video.capture_orientation: Option<u32>`，默认 `None`（自动检测）

- [ ] **[hardcoded]** `src/scrcpy/server.rs:103-104`: `max_size: 1600`, `max_fps: 60` 硬编码  
  **解法**: 使用 `config.video.max_size` 和 `config.video.max_fps`

### 协议与兼容性

- [ ] **[compat]** `src/controller/adb.rs:89-95`: `get_screen_orientation` 仅识别 `ROTATION_*` 字符串  
  **解法**: 添加正则解析数字形式（如 `mCurrentRotation=1`）兼容 Android 6-8

- [ ] **[error-handling]** `src/controller/adb.rs:301`: `remove_reverse_tunnel` 不区分"隧道不存在"和真实错误  
  **解法**: 检查 stderr 包含 `"not found"` 或 `"No such reverse"` 时返回 `Ok(())`

- [ ] **[ipv6]** `src/scrcpy/connection.rs:125`: 硬编码 `127.0.0.1`，无 IPv6 支持  
  **解法**: 改为 `[::1]` 或添加配置 `network.bind_address`

### 文档缺失

- [ ] **[api-docs]** 公开 API 缺少文档：`ScrcpyConnection`, `StreamPlayer`, `VideoDecoder` trait  
  **解法**: 为所有 `pub struct/trait/enum` 添加文档注释（行为、错误条件、线程安全性）

---

## P2 - 重要改进（代码质量与可维护性）

### 代码结构

- [ ] **[refactor]** `src/saide/ui/saide.rs`: 1146 行超大文件  
  **解法**: 拆分为 `state.rs`（状态定义）、`events.rs`（事件处理）、`render.rs`（UI 渲染）、`lifecycle.rs`（初始化/关闭）

- [ ] **[dup]** `src/decoder/{h264,nvdec,vaapi}.rs`: FFmpeg 初始化代码重复（`packet.data_mut().unwrap().copy_from_slice`）  
  **解法**: 抽取 `ffmpeg_utils::send_packet(decoder, data)` 公共函数

- [ ] **[dup]** `src/scrcpy/protocol/control.rs:343,374,406,412,429`: 多处 `.serialize(&mut buf).unwrap()`  
  **解法**: 创建宏 `serialize_msg!(msg) -> Result<Vec<u8>>` 统一错误处理

### 配置化

- [ ] **[config]** `src/controller/mouse.rs:22`: 长按时间硬编码 `LONG_PRESS_DURATION_MS = 300`  
  **解法**: 在 `SAideConfig` 新增 `input.long_press_ms: u64`

- [ ] **[config]** `src/decoder/audio/opus.rs:21`: `SCRCPY_FRAME_SAMPLES = 960` 硬编码  
  **解法**: 根据采样率动态计算 `sample_rate * 0.020`（20ms 帧）

- [ ] **[config]** `src/gpu/mod.rs:41-50`: `nvidia-smi` 失败静默忽略  
  **解法**: 使用 `debug!("nvidia-smi not found: {e}")` 记录日志

- [ ] **[config]** `src/constant.rs:46-51`: 音频缓冲设置硬编码（128 帧，5760 容量）  
  **解法**: 添加专家配置 `audio.buffer_frames` 和 `audio.ring_capacity`（默认值保持不变）

### 协议健壮性

- [ ] **[validation]** `src/scrcpy/protocol/control.rs`: 序列化未检查缓冲区溢出  
  **解法**: 添加 `fn serialize_with_size_check(msg, buf) -> Result<usize>` 辅助函数

- [ ] **[validation]** `src/scrcpy/protocol/video.rs`, `audio.rs`: 反序列化未验证缓冲区长度  
  **解法**: 在读取前检查 `buf.len() >= MIN_PACKET_SIZE`

- [ ] **[unsafe]** `src/decoder/nvdec.rs:72`: `get_cuda_format` 回调裸指针循环无边界检查  
  **解法**: 添加 `assert!(n < 8)` 或使用 `slice::from_raw_parts(fmts, n as usize)`

### 日志与诊断

- [ ] **[i18n]** `src/i18n/fs_source.rs:39-40`: 找不到 i18n 目录时静默降级  
  **解法**: 返回 `Err(I18nError::BundleNotFound { path })` 让调用方决定策略

- [ ] **[i18n]** `src/i18n/fs_source.rs:127-128`: `RecommendedWatcher::new(..).expect(..)`  
  **解法**: 改为 `warn!("i18n hot reload disabled: {e}")` 并继续（release 无需 watch）

- [ ] **[i18n]** `src/i18n/manager.rs:50,156`: 双重 `unwrap_or_else` 嵌套可读性差  
  **解法**: 使用 `?` 操作符或提前返回模式简化

### 资源管理

- [ ] **[cleanup]** `src/scrcpy/connection.rs:317-374`: 同时存在 `shutdown()` 和 `Drop` 实现  
  **解法**: 明确文档说明调用约定：优先显式 `shutdown()`，`Drop` 仅作兜底

- [ ] **[sync]** 混用 `parking_lot::Mutex`, `std::sync`, `RefCell` 无统一策略  
  **解法**: 制定同步原语使用规范（单线程用 `RefCell`，多线程用 `parking_lot::Mutex`）

---

## P3 - 功能增强（新特性）

### UI 完善

- [ ] **[ui]** `src/saide/ui/log.rs`: 实现日志查看器（当前占位）  
  **需求**: 集成 `tracing-appender`，显示最近 1000 行日志，支持级别过滤（ERROR/WARN/INFO/DEBUG）

- [ ] **[ui]** `src/saide/ui/settings.rs`: 实现设置面板（当前占位）  
  **需求**: 可视化配置 GPU 后端、视频编码器、音频、按键映射等（同步到 `config.toml`）

- [ ] **[ui]** `src/saide/ui/overlay.rs`: 实现按键映射叠加层（当前占位）  
  **需求**: 半透明显示当前激活的按键映射位置（类似游戏辅助）

- [ ] **[ux]** `src/saide/ui/mapping.rs`: 缺少保存/加载/导出映射配置  
  **需求**: 支持导入/导出 JSON 格式映射文件，预设常见游戏模板

- [ ] **[ux]** `src/saide/ui/indicator.rs`: 仅显示 FPS  
  **需求**: 增加分辨率、音频状态（采样率/延迟）、连接状态（USB/WiFi）、丢帧计数

### 解码器

- [ ] **[decoder]** `src/decoder/auto.rs`: 解码器选择策略硬编码  
  **需求**: 配置 `gpu.decoder_priority = ["vaapi", "nvdec", "software"]` 允许用户覆盖

- [ ] **[decoder]** `src/scrcpy/hwcodec.rs:104`: `list_video_encoders` 空实现  
  **需求**: 通过 scrcpy-server 查询设备编解码器列表（H.264/H.265/AV1）

### 延迟优化 (Phase 2-3) - 见 docs/LATENCY_OPTIMIZATION.md

- [ ] **[latency]** 零拷贝 GPU 解码 (NVDEC/VAAPI)  
  **需求**: 解码器直接输出 GPU 纹理,在着色器完成 YUV→RGBA 转换

- [ ] **[latency]** CPAL 独占模式 (`src/decoder/audio/player.rs`)  
  **需求**: 尝试音频独占模式降低系统混音缓冲延迟

- [ ] **[latency]** 原始输入设备监听 (新建 `src/input/raw_device.rs`)  
  **需求**: Linux evdev 直接读取输入,绕过 egui 事件循环

- [ ] **[latency]** 鼠标移动速度自适应 (`src/controller/mouse.rs:95`)  
  **需求**: 根据移动速度动态调整拖拽更新间隔

### 平台支持

- [ ] **[platform]** `src/gpu/mod.rs`: 仅支持 Linux DRM 设备检测  
  **需求**: 增加 macOS（IOKit）、Windows（DXGI）GPU 检测支持

---

## P4 - 测试覆盖（质量保障）

### 单元测试缺失

- [ ] **[test]** `src/decoder/vaapi.rs`: 7 个 unsafe 块，无测试覆盖  
  **需求**: 模拟 FFmpeg 回调场景，测试格式协商逻辑

- [ ] **[test]** `src/decoder/nvdec.rs`: 7 个 unsafe 块，无测试覆盖  
  **需求**: 模拟 CUDA 格式选择，测试错误路径

- [ ] **[test]** `src/decoder/h264.rs`: 无测试  
  **需求**: 测试软件解码器初始化、帧损坏处理

- [ ] **[test]** `src/controller/keyboard.rs`: 445 行复杂映射逻辑，无测试  
  **需求**: 测试 Android keycode 映射、修饰键组合（Shift+A）

- [ ] **[test]** `src/controller/mouse.rs`: 357 行鼠标映射逻辑，无测试  
  **需求**: 测试拖拽检测、长按触发、坐标转换

- [ ] **[test]** `src/i18n/fs_source.rs`: 无测试  
  **需求**: 测试文件监听、热重载、语言切换

### 集成测试缺失

- [ ] **[test]** 缺少真实设备连接测试  
  **需求**: Mock scrcpy-server 或录制回放（保存二进制流到文件）

- [ ] **[test]** 缺少 ADB 失败场景测试  
  **需求**: 模拟设备断开、权限不足、多设备冲突

- [ ] **[test]** 缺少网络断线重连测试  
  **需求**: 模拟 TCP 连接中断、ADB 隧道失效

### Fuzzing & 基准测试

- [ ] **[test]** 引入 `cargo-fuzz` 测试协议解析（防崩溃/内存泄漏）  
  **需求**: Fuzz `VideoPacket::from_bytes()`, `AudioPacket::from_bytes()`

- [ ] **[test]** 添加 `criterion` 基准测试  
  **需求**: 测试解码延迟（目标 <16ms）、坐标转换性能（目标 <1μs）

---

## P5 - 文档与规范（长期维护）

### 缺失文档

- [ ] **[docs]** 创建 `docs/architecture.md` 描述模块依赖关系  
  **需求**: 使用 Mermaid 图展示 UI → Controller → Scrcpy → Decoder 层次

- [ ] **[docs]** 创建 `docs/pitfalls.md` 记录开发中遇到的坑  
  **需求**: 记录 FFmpeg 回调安全性、ADB 隧道时序、egui 渲染陷阱等

- [ ] **[docs]** 创建 `docs/protocol.md` 描述 Scrcpy 协议实现  
  **需求**: 记录与官方 scrcpy 的差异、扩展字段

- [ ] **[docs]** `README.md` 缺失  
  **需求**: 提供中英文 README（构建方法、功能特性、截图）

### 代码规范

- [ ] **[lint]** `src/decoder/h264_parser.rs:7-9`: `#[allow(dead_code)]` 标记的函数未使用  
  **解法**: 删除或实际使用这些 import 函数

- [ ] **[lint]** 统一错误处理模式：部分用 `thiserror`, 部分用手动 `Display` impl  
  **解法**: 全面使用 `thiserror` 简化错误定义

---

## 已完成

- [x] 2026-01-15: ✅ **Phase 1 延迟优化完成** (commits 4ea165a → b259ea4 + 本次修复, 共 10 commits)
  - [x] LatencyProfiler + LatencyStats 实现 (追踪 5 阶段 + 60帧统计)
  - [x] FFmpeg 解码器优化 (FAST + EXPERIMENTAL 标志)
  - [x] TCP_QUICKACK 网络优化 (Linux, 3-5ms)
  - [x] ControlSender flush 移除 (2-5ms)
  - [x] 动态 AV 同步阈值 (JitterEstimator, 3-5ms)
  - [x] UI 集成准备 (VideoStats 扩展 + 指示器显示)
  - [x] **视频解码线程集成** (mark_receive/decode/upload/display, 实时统计)
  - [x] **LatencyStats 传递到 UI** (avg/p95 延迟显示)
  - [x] **修复解码时间测量** (mark_decode 调用时机,从解码前移至解码后)
  - [x] **文档化已知限制** (GPU 上传时间为近似值,详见 docs/LATENCY_OPTIMIZATION.md)
  - 音频线程集成已推迟 (低优先级 - 音频基线 <10ms)
  
  **已知限制**: GPU Upload 时间为近似值 (测量通道发送而非实际 GPU 上传,误差 1-3ms)  
  **预期收益**: 13-25ms 延迟降低 (一旦在真机测试验证)  
  **下一步**: Phase 2 - 零拷贝 GPU 解码 (预期额外降低 20-40ms)


- [x] 2026-01-14: 延迟优化路线图 (`docs/LATENCY_OPTIMIZATION.md`)
- [x] 2026-01-14: 音视频回放和输入映射延迟分析
- [x] 2026-01-13: 全代码深度分析（架构、代码质量、测试覆盖、文档）
- [x] 2026-01-12: TODO.md 重写（P0-P4 分级）
- [x] 2026-01-10: i18n 架构重构（debug 热重载 + release 嵌入）
- [x] 2026-01-09: 键盘/鼠标映射器 toggle 快捷键（F10）
- [x] 2026-01-08: CJK 字体支持
- [x] 2026-01-05: 配置格式从 JSON 迁移回 TOML
- [x] 2025-12-20: 删除 video-driven 架构遗留代码

---

## 优先级说明

- **P0**: 可能导致崩溃或数据丢失，必须立即修复
- **P1**: 影响稳定性或严重影响用户体验，应尽快解决
- **P2**: 代码质量问题，影响长期可维护性，计划内解决
- **P3**: 功能增强，可选特性，按需实现
- **P4**: 测试覆盖，质量保障，持续改进
- **P5**: 文档与规范，长期维护，逐步完善

**建议处理顺序**: 
1. **延迟优化 Phase 1** (当前进行中)
2. P0 崩溃风险修复
3. P1 架构设计问题
4. **延迟优化 Phase 2-3**
5. P2 代码质量提升
6. P4 核心模块测试
7. P3 功能增强
