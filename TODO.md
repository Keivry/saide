# SAide 任务清单

> **最后更新**: 2026-01-15  
> **当前状态**: 🚀 Phase 3 延迟优化进行中 (音频 + 输入优化)  
> **Phase 1**: ✅ 完成 (13-25ms 降低, 目标 20-35ms 已达成)  
> **Phase 2**: ❌ 终止 (wgpu v28 无外部内存 API, 详见 `docs/PHASE2_ZEROCOPY_FEASIBILITY.md`)  
> **Phase 3**: 🔄 进行中 (目标额外降低 8-18ms)

---

## P0 - 崩溃风险（必须立即修复）

### 错误处理 & Panic 风险

- [x] **[panic]** `src/constant.rs:11`: `ProjectDirs::from(..).expect(..)` 在非标准环境（Docker/沙盒）会直接 panic  
      ✅ **已修复** (commit d59dca5): 改为 `config_dir() -> Option<PathBuf>` + fallback 到 `/tmp/saide`
- [x] **[panic]** `src/config/mod.rs:121`: `path.to_str().unwrap()` 在非 UTF-8 路径（Windows 特殊字符）会 panic  
      ✅ **已修复** (commit 23ea5f7): 使用 `to_string_lossy()` 处理非 UTF-8 路径

- [x] **[panic]** `src/scrcpy/connection.rs:190-192`: `try_into().unwrap()` 无错误上下文  
      ✅ **已修复** (commit 673e8ee): 改用 `expect("BUG: ...")` 并标注不变式

- [x] **[panic]** `src/saide/ui/saide.rs:454,622,685,702,799,831,868`: 多处 `keyboard_mapper.unwrap()` 和 `mouse_mapper.unwrap()`  
      ✅ **已修复** (commit 9e3b9a6): 使用 `let Some(...) else` 模式 + 早期返回

- [x] **[panic]** `src/saide/coords.rs:152,194,275,326`: `unreachable!()` 在取模运算后（理论上可达，因为整数溢出）  
      ✅ **已修复** (commit 7c6a4fa): 改用 `debug_assert!` + % 4 normalization + fallback

- [x] **[panic]** `src/decoder/audio/player.rs:95`: cpal 音频回调中包含潜在 panic 点  
      ✅ **已修复** (commit 5f6b793): 添加 bounds checking 防止数组越界

### 错误链丢失

- [x] **[error-source]** `src/error.rs:18-46`: `IoError` 未实现 `std::error::Error::source()`，丢失错误链  
      ✅ **已修复** (commit 1834c36): 存储 `Option<Box<io::Error>>` + 实现 `source()` 方法

### 资源管理

- [x] **[blocking-io]** `src/scrcpy/connection.rs:86`: `connect` 签名为 `async` 但内部使用阻塞 I/O（`TcpListener::accept`）  
      ✅ **已修复** (commit 8edf92c): 移除 `async` 签名，统一为同步 API

- [x] **[validation]** ADB 命令路径未验证：多处 `Command::new("adb")` 假设 ADB 在 PATH 中  
      ✅ **已修复** (commit ec5596e): 添加 `AdbShell::verify_adb_available()`，在启动时验证

---

## P1 - 严重问题（影响稳定性或用户体验）

### 架构设计

- [x] **[god-object]** `src/saide/ui/saide.rs:58-138`: `SAideApp` 包含 40+ 字段，违反单一职责原则  
      ✅ **已修复** (commit f2af6d5): 拆分为 `AppState` (8字段)、`UIState` (5字段)、`ConfigState` (6字段)

- [x] **[coupling]** `src/saide/init.rs`: 混合连接建立、设备监控、输入映射初始化三种职责  
      ✅ **已修复**: 拆分为 `ConnectionService` (230行), `DeviceMonitor` (215行), init.rs 简化为协调器 (155行, -67%)

- [x] **[coupling]** `src/saide/ui/player.rs:453-728`: `stream_worker` 275 行单体函数，混合解码器初始化、音频线程、视频循环  
      ✅ **已修复**: 拆分为 `DecoderManager::init()`, `AudioThread::spawn()`, `VideoLoop::run()`, stream_worker 简化为协调器 (310行→74行, -76%)

- [x] **[abstraction]** `src/scrcpy/connection.rs:46-76`: `ScrcpyConnection` 公开原始 `TcpStream`  
      ✅ **已修复** (commit f41bcbd): 字段改为私有，仅暴露 `take_video_stream()`, `set_control_stream()` 等方法

- [x] **[dependency]** `src/error.rs:11-15`: 错误模块依赖 `decoder` 模块（应反向依赖）  
      ✅ **已修复**: `VideoError`, `AudioError` 已在 `decoder/error.rs` 和 `decoder/audio/error.rs`  
      ✅ **设计合理**: 顶层 `SAideError` 使用 `#[from]` 聚合子错误是标准 Rust 模式

### 配置与状态管理

- [x] **[atomic-write]** `ConfigManager::save()` 缺少原子写机制（当前直接覆盖文件）  
      ✅ **已修复** (commit 072e53d): 写到临时文件 `.toml.tmp` 再原子 `fs::rename()`

- [x] **[hardcoded]** `src/main.rs:18-19`: 窗口默认尺寸硬编码 `1280×720`，忽略 DPI 和屏幕尺寸  
      ✅ **已修复** (commit 18a1065): 添加 `general.window_width/height` 配置项

- [x] **[hardcoded]** `src/saide/init.rs:122`: `capture_orientation` 硬编码为 `Some(0)`  
      ✅ **已修复** (commit 18a1065): 添加 `video.capture_orientation: Option<u32>` 配置项

- [x] **[hardcoded]** `src/scrcpy/server.rs:103-104`: `max_size: 1600`, `max_fps: 60` 硬编码  
      ✅ **已修复** (commit 18a1065): 使用 `config.video.max_size` 和 `config.video.max_fps`

### 协议与兼容性

- [x] **[compat]** `src/controller/adb.rs:89-95`: `get_screen_orientation` 仅识别 `ROTATION_*` 字符串  
      ✅ **已修复** (commit 18a1065): 支持数字形式 `0-3` 兼容旧版 Android

- [x] **[error-handling]** `src/controller/adb.rs:301`: `remove_reverse_tunnel` 不区分"隧道不存在"和真实错误  
      ✅ **已修复** (commit 18a1065): 检查 stderr 包含 `"No such reverse"` 时返回 `Ok(())`

- [x] **[ipv6]** `src/scrcpy/connection.rs:125`: 硬编码 `127.0.0.1`，无 IPv6 支持  
      ✅ **已修复** (commit 18a1065): 添加 `general.bind_address` 配置，支持 `[::1]`

### 文档缺失

- [x] **[api-docs]** 公开 API 缺少文档：`ScrcpyConnection`, `StreamPlayer`, `VideoDecoder` trait  
      ✅ **已修复**: 为核心公开 API 添加完整 rustdoc 文档（行为、错误条件、线程安全性、生命周期）

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

- [x] ~~**[config]** `src/decoder/audio/opus.rs:21`: `SCRCPY_FRAME_SAMPLES = 960` 硬编码~~
      ~~**解法**: 根据采样率动态计算 `sample_rate * 0.020`（20ms 帧）~~ 960 是 scrcpy-server 固定值，保持不变

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

### 延迟优化 Phase 3 (进行中) - 详见 docs/LATENCY_OPTIMIZATION.md

#### Phase 2 调研结果 (已完成)

- [x] **[P0]** Phase 2 零拷贝 GPU 解码技术调研 (2026-01-15)  
      **最终状态**: ❌ **已终止** - wgpu v28 无外部内存 API (commit 33b80ff)  
      **调研成果**:
  - ✅ 850+ 行可行性分析 (`docs/PHASE2_ZEROCOPY_FEASIBILITY.md`)
  - ✅ wgpu-hal 原型测试 (`examples/test_vulkan_import.rs`)
  - ✅ v27/v28 兼容性验证 (均不支持)
  - ✅ 成本效益分析 (ash 重写 2-3 周 vs 12-20ms 收益 = 不值得)

  **终止原因**: wgpu v28 仍无 `Device::as_hal()`,等待官方支持时间线未知  
  **决策**: 转向 Phase 3 音频/输入优化 (8-18ms 收益,1 周成本)

#### Phase 3.1: 音频延迟优化 ✅ COMPLETED (2026-01-15)

- [x] **[TERMINATED]** ~~CPAL 独占模式尝试~~ (`src/decoder/audio/player.rs`)  
      **调查结果**: cpal 0.17 **不支持** WASAPI/ALSA 独占模式 API  
      **证据**:
  - `build_output_stream()` 无 `exclusive_mode` 参数
  - 源码中无平台特定独占访问暴露
  - cpal 定位为跨平台高层抽象,不提供底层硬件控制

  **终止原因**: API 不存在,无法实现  
  **替代方案**: 缓冲区大小优化 (下方任务,已完成)  
  **文档**: `docs/pitfalls.md` § cpal 独占模式限制

- [x] **[COMPLETED]** 音频缓冲降至 64 frames (`src/constant.rs:46`)  
      **实施**:
  - ✅ `AUDIO_BUFFER_FRAMES: 128 → 64` (1.33ms @ 48kHz)
  - ✅ 添加 `AudioConfig.buffer_frames` 配置选项 (可覆盖默认值)
  - ✅ 更新 `AudioPlayer::new()` API (接受 buffer_frames 参数)
  - ✅ 导入修复 + 质量检查通过 (fmt + clippy + test)
  - ✅ 文档化调优指南 (`docs/pitfalls.md` +168 行)

  **Commits**:
  - `feat(latency): Phase 3.1 - reduce audio buffer 128→64 frames` (待 push)
  - `docs(latency): Phase 3.1 - document cpal limitations and tuning` (待 push)

  **预期收益**: 1-2ms 延迟降低  
  **硬件测试**: 待用户执行 (underrun 监控)  
  **配置回退**: 弱系统可设置 `buffer_frames = 128/256`

#### Phase 3.2: 输入延迟优化 (P2 - 可选)

- [ ] **[P2]** evdev 原始输入监听 (新建 `src/input/raw_device.rs`)  
      **目标**: 绕过 egui 事件循环,降低 5-10ms  
      **实施计划**:
  1. 使用 `evdev-rs` 直接读取 `/dev/input/event*`
  2. 实现设备热插拔检测
  3. 添加焦点管理 (仅窗口激活时捕获)
  4. 处理权限问题 (需 udev 规则或 root)

  **预期工作量**: 1 周  
  **风险**: 高 (仅 Linux,复杂度高)  
  **优先级**: P2 (Phase 3.1 完成后评估是否实施)

- [ ] **[P3]** 鼠标速度自适应 (`src/controller/mouse.rs:95`)  
      **目标**: 动态调整更新间隔,降低 2-5ms  
      **实施计划**:
  1. 计算鼠标移动速度 (像素/秒)
  2. 快速移动 → 高频更新 (每帧)
  3. 慢速移动 → 低频更新 (节省带宽)

  **预期工作量**: 1 天  
  **风险**: 低  
  **优先级**: P3 (增量优化,可选)

#### 长期跟踪

- [ ] **[future]** 监控 wgpu v29+ 外部内存 API 更新  
      **行动**: 已在 wgpu GitHub 提 Feature Request  
      **检查频率**: 每季度一次 (2026 Q2, Q3)  
      **触发条件**: 如 wgpu 暴露 `Device::as_hal()`,重新评估 Phase 2  
      **参考**: `docs/PHASE2_ZEROCOPY_FEASIBILITY.md` § 技术方案

---

#### Phase 3 预期成果

```
累计延迟降低:
├─ Phase 1: 13-25ms (✅ 已完成)
├─ Phase 3.1: 4-10ms (音频优化, 进行中)
├─ Phase 3.2: 7-15ms (输入优化, 可选)
└─ 总计: 24-50ms

最终延迟:
├─ 基线: 50-70ms
├─ Phase 1 后: 30-50ms (✅ 目标达成)
├─ Phase 3.1 后: 26-40ms (P1 目标)
└─ Phase 3.2 后: 20-35ms (P2 延伸目标)
```

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

- [x] **[docs]** 创建 `docs/architecture.md` 描述模块依赖关系  
      ✅ **已完成**: 已存在完整架构文档,包含 Mermaid 图和层次结构说明

- [x] **[docs]** 创建 `docs/pitfalls.md` 记录开发中遇到的坑  
      ✅ **已完成**: 已存在完整的坑点文档,记录了 FFmpeg、ADB、音频等各类问题

- [ ] **[docs]** 创建 `docs/protocol.md` 描述 Scrcpy 协议实现  
      **需求**: 记录与官方 scrcpy 的差异、扩展字段 (当前为 `SCRCPY_PROTOCOL.md`)

- [x] **[docs]** `README.md` 缺失  
      ✅ **已完成** (2026-01-16): 创建 README.md (520行) + README.zh.md (520行) + LICENSE
      - 完整项目介绍 (中英双语)
      - 详细安装和使用指南
      - 配置说明和故障排除
      - 性能指标和路线图
      - 开发指南和贡献规范

### 代码规范

- [ ] **[lint]** `src/decoder/h264_parser.rs:7-9`: `#[allow(dead_code)]` 标记的函数未使用  
      **解法**: 删除或实际使用这些 import 函数

- [ ] **[lint]** 统一错误处理模式：部分用 `thiserror`, 部分用手动 `Display` impl  
      **解法**: 全面使用 `thiserror` 简化错误定义

---

## 已完成

### Phase 1: 延迟优化基础设施 (✅ 完成 2026-01-15)

- [x] LatencyProfiler + LatencyStats 实现 (追踪 5 阶段 + 60 帧统计)
- [x] FFmpeg 解码器优化 (FAST + EXPERIMENTAL 标志)
- [x] TCP_QUICKACK 网络优化 (Linux, 3-5ms)
- [x] ControlSender flush 移除 (2-5ms)
- [x] 动态 AV 同步阈值 (JitterEstimator, 3-5ms)
- [x] 视频解码线程集成 (mark_receive/decode/upload/display)
- [x] LatencyStats 传递到 UI (avg/p95 延迟显示)
- [x] 修复解码时间测量 (mark_decode 时机修正)
- [x] 文档化已知限制 (GPU 上传时间近似值)

**成果**: 13-25ms 延迟降低, 目标 30-50ms → 20-35ms ✅ 达成  
**已知限制**: GPU Upload 时间误差 1-3ms (测量通道发送,非实际上传)  
**Commits**: 4ea165a → 76bebac (10 commits)

### Phase 2: 零拷贝 GPU 解码调研 (❌ 终止 2026-01-15)

- [x] 完整可行性分析 (`docs/PHASE2_ZEROCOPY_FEASIBILITY.md`, 850+ 行)
- [x] wgpu-hal 原型测试 (`examples/test_vulkan_import.rs`)
- [x] wgpu v27 验证 (❌ 无 hal 公开 API)
- [x] wgpu v28 验证 (❌ 仍无外部内存 API)
- [x] 成本效益分析 (ash 重写 2-3 周 vs 12-20ms = 不值得)
- [x] 调研文档提交 (commit 33b80ff)

**终止原因**: wgpu 未暴露外部内存导入 API, ash 重写成本过高  
**技术遗产**: 完整设计文档 + 原型代码 (供未来 wgpu v29+ 重新评估)  
**决策**: 转向 Phase 3 音频/输入优化 (更优投入产出比)

### 其他历史任务

- [x] 2026-01-14: 延迟优化路线图 (`docs/LATENCY_OPTIMIZATION.md`)
- [x] 2026-01-14: 音视频回放和输入映射延迟分析
- [x] 2026-01-13: 全代码深度分析 (架构/质量/测试/文档)
- [x] 2026-01-12: TODO.md 重写 (P0-P4 分级)
- [x] 2026-01-10: i18n 架构重构 (debug 热重载 + release 嵌入)
- [x] 2026-01-09: 键盘/鼠标映射器 toggle 快捷键 (F10)
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

1. 🚀 **Phase 3 延迟优化** (当前进行中)
   - Phase 3.1 音频优化 (P1, 预期 1 周)
   - Phase 3.2 输入优化 (P2, 可选)
2. P0 崩溃风险修复
3. P1 架构设计问题
4. P2 代码质量提升
5. P4 核心模块测试
6. P3 功能增强
