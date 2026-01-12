# SAide 项目任务清单

> **最后更新**: 2026-01-12  
> **当前状态**: 核心功能已实现，需系统性提高健壮性与可维护性

---

## 进行中

_无进行中任务，待审核确认后开始_

---

## P0 - 关键问题（可能导致崩溃或核心功能失效）

### 错误处理 & 边界条件

- [ ] **[panic-risk]** `src/constant.rs:11`: `ProjectDirs::from(..).expect(..)` 在非标准环境（容器/沙盒）下直接 panic  
  **解法**: 改为返回 `Result`，在 `main` 中提供降级路径（如临时目录）
- [ ] **[panic-risk]** `src/config/mod.rs:121`: `path.to_str().unwrap()` 在非 UTF-8 路径下 panic  
  **解法**: 使用 `to_string_lossy()` 或返回 `ConfigError::InvalidPath`
- [ ] **[panic-risk]** `src/scrcpy/connection.rs:190-192`: `try_into().unwrap()` 无上下文信息  
  **解法**: 改用 `expect("Failed to parse codec metadata")`
- [ ] **[error-loss]** `src/error.rs`: `IoError` 未实现 `std::error::Error::source()`，丢失错误链  
  **解法**: 添加 `source()` 方法返回原始 `io::Error`（需修改为 `Box<io::Error>`）

### 资源管理

- [ ] **[connection]** `src/scrcpy/connection.rs`: `connect` 签名为 `async` 但内部使用阻塞 I/O（`TcpListener::accept`）  
  **解法**: 统一为同步 API（移除 `async`）或完全改用 `tokio::net::TcpListener`
- [ ] **[adb-path]** ADB 命令路径未验证：多处 `Command::new("adb")` 假设 ADB 在 PATH 中  
  **解法**: 在 `AdbShell::new()` 验证 `adb` 可执行性，缓存路径或提供配置覆盖

---

## P1 - 重要问题（影响稳定性或用户体验）

### 配置与状态管理

- [ ] **[config]** `ConfigManager::save()` 缺少原子写机制（当前直接覆盖文件）  
  **解法**: 写到临时文件再 `rename()`（符合 POSIX 原子性）
- [ ] **[ux]** `src/main.rs:18-19`: 窗口默认尺寸硬编码 1280×720，忽略 DPI 和屏幕尺寸  
  **解法**: 从配置读取或基于主屏幕尺寸动态计算（如 80% 高度）
- [ ] **[ux]** `src/saide/init.rs:122`: `capture_orientation` 硬编码为 `Some(0)`  
  **解法**: 在 `SAideConfig` 新增 `capture_orientation: Option<u32>`，默认 `None`（自动检测）

### 协议与兼容性

- [ ] **[compat]** `src/controller/adb.rs:89-95`: `get_screen_orientation` 仅识别 `ROTATION_*` 字符串  
  **解法**: 添加正则解析数字形式（如 `mCurrentRotation=1`）兼容旧 Android 版本
- [ ] **[error-handling]** `src/controller/adb.rs:301`: `remove_reverse_tunnel` 不区分"隧道不存在"和真实错误  
  **解法**: 检查 stderr 包含 `"not found"` 时返回 `Ok(())`
- [ ] **[ipv6]** `src/scrcpy/connection.rs:125`: 硬编码 `127.0.0.1`，无 IPv6 支持  
  **解法**: 改为 `[::1]` 或配置项选择 IPv4/IPv6

### 错误处理

- [ ] **[panic-risk]** `src/saide/coords.rs:152,194,275,326`: `unreachable!()` 在取模运算后（理论上可达）  
  **解法**: 改用 `debug_assert!` + `saturating_sub` 或返回 `Result`
- [ ] **[panic-risk]** `src/decoder/audio/player.rs`: cpal 回调中 `unwrap` 可能导致音频线程 panic  
  **解法**: 使用 `if let Ok(...) = ...` 并记录错误计数
- [ ] **[validation]** `src/saide/init.rs:83-90`: ADB 启动仅检查 `is_ok()`，忽略退出码和 stderr  
  **解法**: 检查 `status.code() == Some(0)` 并解析 stderr（如"daemon not running"）

---

## P2 - 改进项（代码质量与长期可维护性）

### 代码结构

- [ ] **[refactor]** `src/saide/ui/saide.rs`: 1146 行超大文件  
  **解法**: 拆分为 `state.rs`、`events.rs`、`render.rs`、`lifecycle.rs`
- [ ] **[dup]** `src/decoder/{h264,nvdec,vaapi}.rs`: FFmpeg 初始化代码重复  
  **解法**: 抽取 `ffmpeg_utils::init_decoder(codec_id, width, height)` 公共函数

### 配置化

- [ ] **[config]** `src/controller/mouse.rs:19-25`: 拖拽阈值/长按时间硬编码  
  **解法**: 在 `SAideConfig` 新增 `input.drag_threshold`、`input.long_press_ms`
- [ ] **[config]** `src/decoder/audio/opus.rs:21`: `SCRCPY_FRAME_SAMPLES = 960` 硬编码  
  **解法**: 根据采样率动态计算 `sample_rate * 0.020`（20ms 帧）
- [ ] **[config]** `src/gpu/mod.rs:41-50`: `nvidia-smi` 失败静默忽略  
  **解法**: 使用 `debug!("nvidia-smi not found: {e}")` 记录

### 协议健壮性

- [ ] **[validation]** `src/scrcpy/protocol/control.rs`: 多处 `msg.serialize(&mut buf).unwrap()`  
  **解法**: 抽取 `serialize_with_size(msg) -> Result<Vec<u8>>` 辅助函数
- [ ] **[validation]** `src/scrcpy/protocol/control.rs:350-366...`: 反序列化未验证缓冲区长度  
  **解法**: 在读取前检查 `buf.len() >= expected_size`
- [ ] **[unsafe]** `src/decoder/nvdec.rs:72`: `get_cuda_format` 回调裸指针循环无边界检查  
  **解法**: 添加 `assert!(n < MAX_PIXEL_FORMATS)` 或使用 `slice::from_raw_parts`

### 日志与诊断

- [ ] **[i18n]** `src/i18n/fs_source.rs:39-40`: 找不到 i18n 目录时静默降级  
  **解法**: 返回 `Err(I18nError::BundleNotFound)` 让调用方决定降级策略
- [ ] **[i18n]** `src/i18n/fs_source.rs:127-128`: `RecommendedWatcher::new(..).expect(..)`  
  **解法**: 改为 `warn!("Hot reload disabled: {e}")` 并继续（release 无需 watch）
- [ ] **[i18n]** `src/i18n/manager.rs:50,156`: 双重 `unwrap_or_else` 嵌套  
  **解法**: 使用 `?` 操作符简化

---

## P3 - 功能增强（新特性）

### UI 完善

- [ ] **[ui]** `src/saide/ui/log.rs`: 实现日志查看器（当前占位）  
  **需求**: 集成 `tracing-appender`，显示最近 1000 行日志，支持过滤
- [ ] **[ui]** `src/saide/ui/settings.rs`: 实现设置面板（当前占位）  
  **需求**: 可视化配置 GPU、音视频、映射等（同步到 `config.toml`）
- [ ] **[ui]** `src/saide/ui/overlay.rs`: 实现按键映射叠加层（当前占位）  
  **需求**: 半透明显示当前激活的按键映射位置
- [ ] **[ux]** `src/saide/ui/mapping.rs`: 缺少保存/加载/导出映射配置  
  **需求**: 支持导入/导出 JSON 格式映射文件
- [ ] **[ux]** `src/saide/ui/indicator.rs`: 仅显示 FPS  
  **需求**: 增加分辨率、音频状态（采样率/延迟）、连接状态

### 解码器

- [ ] **[decoder]** `src/decoder/auto.rs`: 解码器选择策略硬编码  
  **需求**: 配置 `gpu.decoder_priority = ["vaapi", "nvdec", "software"]`
- [ ] **[decoder]** `src/scrcpy/hwcodec.rs:104`: `list_video_encoders` 空实现  
  **需求**: 通过 scrcpy-server 查询设备编解码器列表
- [ ] **[latency]** `src/saide/ui/player.rs:374`: `latency_ms` 固定为 0  
  **需求**: 实现 PTS - 系统时间的端到端延迟估算

### 平台支持

- [ ] **[platform]** `src/gpu/mod.rs`: 仅支持 Linux DRM 设备  
  **需求**: 增加 macOS（IOKit）、Windows（DXGI）GPU 检测

---

## P4 - 测试覆盖（质量保障）

### 集成测试

- [ ] 缺少真实设备连接测试（需 mock scrcpy-server 或录制回放）
- [ ] 缺少 ADB 失败场景测试（设备断开、权限不足、多设备冲突）
- [ ] 缺少网络断线重连测试

### 单元测试

- [ ] `controller/adb.rs`: 无异常输出解析测试（如旧版 Android）
- [ ] `decoder/*`: 无帧损坏/分辨率切换测试
- [ ] `scrcpy/connection.rs`: 无超时/部分连接测试
- [ ] `scrcpy/protocol/video.rs:100-101...`: 测试代码重复，需抽取辅助函数

### Fuzzing & Bench

- [ ] 引入 `cargo-fuzz` 测试协议解析（防崩溃/内存泄漏）
- [ ] 添加 `criterion` 基准测试（解码延迟、坐标转换性能）

---

## 已完成

- [x] 2026-01-12: 全代码分析与 TODO.md 重写（P0-P4 分级）
- [x] 2026-01-10: i18n 架构重构（debug 热重载 + release 嵌入）
- [x] 2026-01-09: 键盘/鼠标映射器 toggle 快捷键（F10）
- [x] 2026-01-08: CJK 字体支持
- [x] 2026-01-05: 配置格式从 JSON 迁移回 TOML
- [x] 2025-12-20: 删除 video-driven 架构遗留代码

---

**注**: 优先级基于影响范围（crash > 功能失效 > UX 缺陷 > 技术债），请从 P0 开始逐项修复
