# 项目任务清单

## 进行中

- [ ] **阶段 3**: 输入事件映射（低延迟控制）
- [ ] **阶段 4**: 音频流支持  
- [ ] **阶段 5**: 性能调优与稳定性

## 待开始

- [ ] 帧缓冲管理与渲染集成（UI 层面）
- [ ] VAAPI 硬件解码支持
- [ ] 延迟优化

## 已完成

- [x] **修复构建配置** (commit: 待提交)
  - [x] Cargo.toml: 修正 example 名称 `test_decoder` → `test_decode_video`
  - [x] 修复 clippy 警告（collapsible_if, unwrap after is_some）
  - [x] 清理未使用导出

- [x] **渲染管线集成** (commit: 待提交)
  - [x] 创建 `decoder/rgba_render.rs` (206 行)
  - [x] 创建 `decoder/rgba_shader.wgsl` (31 行)
  - [x] RGBA 纹理上传到 wgpu
  - [x] 完整渲染管线（类似 YUV）

- [x] **阶段 1**: Scrcpy 协议实现 (commit: a7816f6, 78c8863, 1ecade7)
  - [x] 协议解析（控制 + 视频）
  - [x] Server 连接管理
  - [x] 元数据处理（device + codec）
  - [x] 真实设备测试验证（10/10 包成功）
  
- [x] **阶段 2**: 视频解码 (commit: 0ef9048)
  - [x] FFmpeg H.264 解码器集成
  - [x] RGBA 帧输出
  - [x] 真实设备解码测试（5/5 帧成功）
  - [x] PPM 图片导出验证

## 技术亮点

- ✅ 成功解析 Scrcpy 3.3.3 协议
- ✅ 完整的 H.264 解码流程（SPS/PPS → IDR → P-frame）
- ✅ 1920x1080@60fps RGBA 输出
- ✅ 真实设备测试通过
- ✅ wgpu 渲染管线集成（RGBA 纹理直接上传）
