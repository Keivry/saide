# Saide 性能优化指南

## 当前已实施的优化 ✅

### 1. 视频编码优化（Android 端）
- ✅ Baseline Profile (`profile=1,level=1`) - 减少 16-33ms
- ✅ 自动硬件编码器检测 (MTK/Qualcomm/Exynos)
- ✅ 低延迟编码器配置

### 2. 网络传输优化
- ✅ TCP_NODELAY (禁用 Nagle 算法) - 减少 5-10ms
- ✅ 直接 TCP 传输（无中间层）

### 3. 解码器优化（PC 端）
- ✅ AV_CODEC_FLAG_LOW_DELAY - 减少 10-20ms
- ✅ 单线程解码 (thread_count=1)
- ✅ VAAPI/NVDEC 硬件加速

### 4. 渲染优化
- ✅ VSync 可配置关闭 (config.toml: `vsync = false`) - 减少 8-26ms
- ✅ NV12 零拷贝纹理上传
- ✅ 最小化 CPU-GPU 数据传输
- ✅ 帧缓冲优化 (FRAME_BUFFER_SIZE = 1) - 减少 16-32ms
- ✅ 主动帧丢弃策略（只保留最新帧）

### 5. 输入优化
- ✅ scrcpy 控制通道（直接发送，无 adb shell）- 减少 40-90ms
- ✅ 异步输入处理

**总计已优化**: ~95-211ms

---

## 配置建议

### 低延迟配置 (config.toml)
```toml
[scrcpy.video]
bit_rate = "24M"          # 高码率保证画质
max_fps = 60              # 高帧率
max_size = 1280           # 适中分辨率（1280-1920）
codec = "h264"            # H.264 延迟最低

[gpu]
vsync = false             # 关闭 VSync（重要！）
backend = "VULKAN"        # 推荐 Vulkan（Linux）或 DX12（Windows）

[scrcpy.options]
turn_screen_off = true    # 关闭设备屏幕可降低编码延迟
stay_awake = true
```

### 硬件加速选择
- **Intel GPU**: 自动使用 VAAPI
- **NVIDIA GPU**: 自动使用 NVDEC（capture_orientation 自动锁定）
- **其他**: 回退到软件解码

---

## 进一步优化建议

### 1. 可选优化（需手动实施）

#### A. 禁用 frame metadata（如不需要 PTS）
**修改**: `src/app/init.rs`
```rust
params.send_frame_meta = false;  // 当前为 true
```
**预期收益**: -2-5ms
**风险**: 无法使用 AV 同步功能

#### B. 降低分辨率
**配置**: `max_size = 1024` (当前 1280)
**预期收益**: -5-10ms (编码+网络+解码)
**代价**: 画质下降

#### C. 降低帧率
**配置**: `max_fps = 30` (当前 60)
**预期收益**: -20-40ms (平均延迟)
**代价**: 流畅度下降

### 2. 高级优化（需要大量开发）

#### GPU 零拷贝
**复杂度**: 🔴 高
**需要**: wgpu unsafe API + DMA-BUF + VAAPI 集成
**预期收益**: -8-10ms
**风险**: 兼容性问题

---

## 延迟测量

### 理论最佳延迟（60 FPS，当前配置）
- 编码: 16ms (1 帧)
- 网络: 5ms
- 解码: 10ms
- 渲染: 8ms (vsync off)
- **总计**: ~40ms

### 实际延迟（预估）
- **最佳情况**: 50-70ms
- **典型情况**: 70-100ms
- **目标**: <70ms

### 测量方法
1. 在设备屏幕显示秒表
2. 在 PC 录屏（高帧率相机）
3. 对比设备屏幕和 PC 显示的时间差

---

## 故障排查

### 高延迟（>150ms）
- [ ] 检查 `vsync = false` 是否生效
- [ ] 确认使用硬件加速（VAAPI/NVDEC）
- [ ] 检查网络延迟（WiFi vs USB）
- [ ] 关闭设备屏幕（`turn_screen_off = true`）

### 卡顿/丢帧
- [ ] 降低 `max_fps` 或 `max_size`
- [ ] 检查 CPU/GPU 占用
- [ ] 尝试不同的编码器（自动检测可能不是最优）

### 输入延迟高
- [ ] 确认使用 scrcpy 控制通道（不是 adb shell）
- [ ] 检查设备是否进入省电模式

---

**最后更新**: 2025-12-16
