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
- [x] **设备 Codec Options 自动检测与缓存** (commit: f02c9d1)
  - 问题：不同设备支持的 video_codec_options 差异巨大
  - 实现：二分测试法 + JSON 配置缓存（`~/.config/saide/device_profiles.json`）
  - 工具：`cargo run --example probe_codec [serial]`
  - 已验证：Kirin 980 (0/8)，MTK mt6991 有待测试

## 进行中 🔄
- [ ] 测试硬件编码器对延迟的实际影响
- [ ] 添加延迟测量工具

### 待实现 📋
- [ ] GPU 零拷贝 (VAAPI → DMA-BUF → wgpu)
  - 复杂度：高
  - 预期收益：8-10ms
- [ ] 编码器参数优化 (bframes=0, profile=baseline)
- [ ] 缓冲深度优化

---

**相关文档**：见 `FINDINGS.md`
