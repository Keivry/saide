# 项目任务清单

## 已完成 ✅

- [x] Scrcpy 协议实现与测试 (commit: 55a121c)
- [x] H.264 软件解码器 (H264Decoder)
- [x] RGBA 渲染管线 (完美工作)
- [x] 真实设备渲染示例 (render_device)
- [x] 屏幕旋转支持 (动态分辨率切换)
- [x] RGBA 着色器顶点修复
- [x] 所有核心功能测试通过 (16/16)

## 进行中 🚧

### VAAPI 硬件加速 (实验性)
- [x] VAAPI 解码器基础实现
- [x] NV12 渲染管线框架
- [x] 标准 BT.601 转换着色器
- [ ] **调试 VAAPI 解码错误** (issue: 23, 高优先级)
- [ ] **修复 NV12 渲染条纹问题**
- [ ] 参考 nokhwa/webrtc.rs 等成熟实现

### 备选方案
- [ ] VAAPI 解码 + sws_scale → RGBA (更稳定)
- [ ] 复用现有 RGBA 渲染管线

## 待开始 📋

### 性能优化
- [ ] 端到端延迟测量
- [ ] 帧率统计与显示
- [ ] CPU/GPU 占用监控

### 用户体验
- [ ] 中英文双语 README
- [ ] 命令行参数支持
- [ ] 配置文件系统

### 代码质量
- [ ] Clippy 警告清理
- [ ] 文档完善
- [ ] 示例代码注释

## 参考资源

### wgpu NV12 渲染
- nokhwa (摄像头库): github.com/l1npengtul/nokhwa
- webrtc.rs: WebRTC Rust 实现
- video-rs: FFmpeg 绑定

### VAAPI 调试
- Intel VAAPI 文档
- Mesa VAAPI 驱动
- FFmpeg VAAPI 示例

## 当前推荐使用

**生产环境**:
```bash
cargo run --example render_device
```
- ✅ H264Decoder 软件解码
- ✅ RGBA 渲染管线
- ✅ 完美稳定

**实验测试**:
```bash
cargo run --example render_vaapi  # WIP, 有问题
cargo run --example test_vaapi    # VAAPI 解码测试
```

---

**最后更新**: 2025-12-11 02:24
**版本**: v0.1.0-dev
