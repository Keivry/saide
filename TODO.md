# 项目任务清单

## 最新优化 ✅ (2025-12-11)

### scrcpy 级低延迟优化（预期减少 40-95ms）

#### ✅ 已实施优化

1. **Android 编码侧优化**
   - ✅ Baseline Profile (无 B 帧): `profile=1,level=1`
   - ✅ 自动硬件编码器检测 (MTK/Qualcomm/Exynos)
   - 预期减少: 16-33ms

2. **网络传输优化**
   - ✅ TCP_NODELAY (禁用 Nagle 算法)
   - 预期减少: 5-10ms

3. **PC 解码侧优化**
   - ✅ AV_CODEC_FLAG_LOW_DELAY (VAAPI + 软解)
   - ✅ 单线程解码 (thread_count=1)
   - 预期减少: 10-20ms

4. **渲染优化**
   - ✅ 禁用 VSync (`vsync: false`)
   - ✅ NV12 零拷贝纹理上传
   - 预期减少: 8-26ms

**总计预期减少**: 39-89ms  
**目标端到端延迟**: 40-70ms (对标 scrcpy 的 35-70ms)

#### 📋 未实施（复杂度高）

- [ ] GPU 零拷贝 (VAAPI → DMA-BUF → wgpu)
  - 需要 wgpu unsafe 接口
  - 预期减少: 8-10ms

#### 📚 参考文档

- scrcpy demuxer.c:188 - `AV_CODEC_FLAG_LOW_DELAY`
- scrcpy server.c:688 - `TCP_NODELAY`
- Android MediaCodecInfo.CodecProfileLevel - Baseline Profile

---

## 已完成 ✅

### 核心功能
- [x] Scrcpy 协议完整实现
- [x] H.264 软件解码器 (H264Decoder + RGBA)
- [x] **VAAPI 硬件加速解码器** ✅ NEW
- [x] **NV12 渲染管线** ✅ NEW
- [x] RGBA 渲染管线
- [x] 真实设备渲染示例 (render_device)
- [x] **VAAPI 渲染示例** (render_vaapi) ✅ NEW
- [x] 屏幕旋转支持
- [x] 动态分辨率切换
- [x] 所有单元测试通过 (16/16)

### 最新成就 🎉
- [x] **修复 VAAPI NV12 条纹问题**（linesize padding）
- [x] FFmpeg linesize 正确处理（32 字节对齐）
- [x] 标准 BT.601 YUV→RGB 转换
- [x] 双纹理 NV12 渲染（Y: R8, UV: Rg8）

## 待优化 📋

### 性能与监控
- [ ] 端到端延迟测量
- [ ] 帧率统计与显示
- [ ] CPU/GPU 占用监控
- [ ] VAAPI vs 软件解码性能对比

### 用户体验
- [ ] 中英文双语 README
- [ ] 命令行参数支持（设备选择、分辨率等）
- [ ] 配置文件系统
- [ ] 错误提示优化

### 代码质量
- [ ] 清理未使用字段（scaler, output_format）
- [ ] Clippy 警告修复
- [ ] 文档完善
- [ ] 示例代码注释

## 技术细节

### VAAPI NV12 处理
```
分辨率: 864x1920
Y linesize: 896 (32 bytes padding)
UV linesize: 896 (32 bytes padding)

解决方案：逐行复制移除 padding
for row in 0..height {
    let start = row * linesize;
    let end = start + width;
    data.extend_from_slice(&src[start..end]);
}
```

### 当前可用方案

**硬件加速（推荐）**:
```bash
cargo run --example render_vaapi
```
- ✅ VAAPI H.264 硬件解码
- ✅ NV12 原生渲染
- ✅ 低延迟
- ✅ 低 CPU 占用

**软件渲染（稳定备选）**:
```bash
cargo run --example render_device
```
- ✅ FFmpeg 软件解码
- ✅ RGBA 渲染
- ✅ 兼容性好

## 参考资源

### NV12 渲染
- ChatGPT NV12 着色器参考
- mpv 播放器 YUV 处理
- FFmpeg NV12 格式规范

### VAAPI
- Intel VAAPI 文档
- Mesa VAAPI 驱动
- FFmpeg VAAPI 示例

---

**最后更新**: 2025-12-11 02:47  
**版本**: v0.2.0-dev  
**状态**: 核心功能完成 ✅ 硬件加速完成 ✅

## 延迟优化进展 🚀

### 已完成 ✅
- [x] **硬件编码器自动检测** (commit: bd18dfc)
  - 自动检测设备最佳 H.264 硬件编码器
  - 优先级：c2.android > OMX.qcom > OMX.MTK > OMX.Exynos
  - 预期延迟改善：15-45ms
- [x] **H.264 SPS 解析器支持 High Profile** (commit: f02c9d1)
  - 完整实现 ITU-T H.264 7.3.2.1.1 规范（支持所有 profiles）
  - 修复 MTK 编码器 1920x864 → 32x32 解析错误
- [x] **设备 Codec Options 自动检测与缓存** (commit: d6a3ff5)
  - 问题：不同设备支持的 video_codec_options 差异巨大
  - 实现：直接用 ScrcpyConnection 测试，读取视频包验证
  - 工具：`cargo run --example probe_codec [serial]`
  - 已验证：MTK mt6991 (8/8 全支持)，Kirin 980 (0/8)
- [x] **GPU 自适应 profile 选择** (commit: PENDING)
  - 自动检测 NVIDIA/Intel/AMD GPU
  - VAAPI: `profile=66` (Baseline Profile)
  - NVDEC: `profile=65536` (NVIDIA 特定枚举值)

## 进行中 🔄

### 代码重构 (2025-12-12)

#### 已完成 ✅
- [x] **移除外部 scrcpy 进程依赖**
  - [x] 废弃 `controller/scrcpy.rs`（外部进程管理）
  - [x] 废弃 `app/ui/player.rs`（V4L2Player）
  - [x] 移除初始化流程中的 Scrcpy 启动逻辑
  - [x] 统一使用内部 `StreamPlayer` 实现
  - [x] 代码减少 314 行
- [x] **代码质量提升**
  - [x] 修复所有 Clippy 警告
  - [x] 修复 Doctest 格式错误
  - [x] 优化循环使用迭代器
  - [x] 移除不必要的类型转换
  - [x] 添加 Default trait 实现
  - [x] 34/34 单元测试通过

#### 技术细节
**重构前**：外部 scrcpy 进程 → V4L2 → V4L2Player  
**重构后**：内部 scrcpy 协议 → StreamPlayer（VAAPI/NVDEC）

**收益**：
- ✅ 更简洁的架构（-314 行代码）
- ✅ 更好的性能（无 V4L2 中间层）
- ✅ 更统一的实现（所有示例都使用 StreamPlayer）
- ✅ 更好的可维护性（减少外部依赖）

---

### 音视频同步实现 ✅ (2025-12-12)

#### 已完成
- [x] **AV 同步时钟模块** (`src/sync/clock.rs`)
  - [x] AVClock: PTS → 系统时间映射
  - [x] AVSync: 同步状态管理
  - [x] PTS 定时渲染逻辑
  - [x] 帧丢弃策略（超过阈值）
  - [x] 7/7 单元测试通过
- [x] **AV 同步示例** (`examples/render_avsync.rs`)
  - [x] 视频线程：PTS 定时渲染（VAAPI + NV12）
  - [x] 音频线程：独立缓冲播放（Opus + cpal）
  - [x] egui 主线程：被动接收最新帧
  - [x] 同步状态 UI 显示（V-PTS / A-PTS / Diff）

#### 技术细节
**同步策略（scrcpy-style）：**
```
视频：PTS → Instant 映射 → thread::sleep() → 定时发送到 egui
音频：独立 cpal 线程 + 100-200ms 缓冲（自适应抖动）
同步：共享 AVClock，20ms 阈值，超时丢帧
```

**性能预期：**
- 音视频延迟差：< 20ms（阈值内同步）
- 视频渲染延迟：0ms（PTS 直接映射，无额外缓冲）
- 音频延迟：100-200ms（网络抖动缓冲）

#### 使用方法
```bash
cargo run --example render_avsync [device_serial]
```

---

### 音频支持实现 (已完成)

#### 阶段 1：基础架构 ✅ 已完成
- [x] 添加 `cpal` 音频播放依赖
- [x] 创建 `src/decoder/audio/` 模块结构
  - [x] `mod.rs` - 音频解码器 trait
  - [x] `opus.rs` - Opus 解码器 (FFmpeg)
  - [x] `player.rs` - 音频播放器 (cpal)
- [x] 创建 `src/scrcpy/protocol/audio.rs` 音频包解析
- [x] 所有测试通过 (2/2)

#### 阶段 2：Opus 解码 ✅ 已完成
- [x] 实现 `OpusDecoder` (FFmpeg libopus)
  - [x] 初始化 Opus 解码上下文
  - [x] 解码 Opus 包到 PCM (f32)
  - [x] 处理 EAGAIN（需要更多数据）
- [x] Connection::read_audio_packet() 方法
- [x] 测试示例：`examples/test_audio.rs`
  - [x] 音频流读取
  - [x] Opus 解码
  - [x] 实时播放

#### 阶段 3：音频播放 (已随阶段 1 完成)
- [x] 实现 `AudioPlayer` (cpal)
  - [x] 初始化音频输出设备
  - [x] 创建音频流 (crossbeam ring buffer)
  - [x] 处理缓冲区欠载/溢出
- [x] 测试：播放解码后的 PCM 数据

#### 阶段 4：UI 集成 (待开始)
- [ ] 在主 UI 添加音频控制
  - [ ] 音量滑块
  - [ ] 静音按钮
  - [ ] 音频延迟显示
  - [ ] 缓冲区状态监控
- [ ] 配置文件音频选项
- [ ] 视频+音频同步播放

#### 技术细节参考
- **音频格式**: Opus (默认), AAC, FLAC, RAW
- **采样率**: 48kHz (Android 输出标准)
- **声道**: 立体声 (2 channels)
- **缓冲**: 100ms (当前实现)
- **同步**: PTS 基准与视频对齐（待实现）
- **Android 版本要求**: API 30+ (Android 11+)

#### 已知限制
- ❌ Android 10 及以下不支持音频捕获
- ⚠️ Android 11 需要设备屏幕解锁
- ✅ Android 12+ 开箱即用

---

### 遗留任务

- [ ] 添加延迟测量工具
- [ ] 测试硬件编码器对延迟的实际影响

## 待实现 📋
- [ ] GPU 零拷贝 (VAAPI → DMA-BUF → wgpu)
  - 复杂度：高
  - 预期收益：8-10ms
- [ ] 缓冲深度优化

---

**相关文档**：见 `FINDINGS.md`
