# Saide Examples

## 推荐使用主程序

```bash
cargo run
```

主程序包含完整功能：
- ✅ VAAPI/NVDEC 自动选择
- ✅ 音视频同步播放
- ✅ 完整的 UI 控制
- ✅ 键盘/鼠标映射
- ✅ 配置文件支持

---

## 可用示例

### 1. render_avsync.rs ⭐ 推荐
完整的音视频同步渲染示例

```bash
cargo run --example render_avsync [device_serial]
```

**功能**：
- VAAPI/NVDEC 自动解码
- Opus 音频解码 + cpal 播放
- egui 实时渲染
- AV 同步状态显示

---

### 2. test_connection.rs - 基础连接测试

```bash
cargo run --example test_connection [device_serial]
```

---

### 3. probe_codec.rs - 设备 Codec Options 探测

```bash
cargo run --example probe_codec [device_serial]
```

---

### 4. audio_diagnostic.rs - 音频诊断

```bash
cargo run --example audio_diagnostic
```

---

## 已归档示例

以下示例已移动到 `examples/deprecated/`（功能已被主程序覆盖）：

- ~~render_device.rs~~ → 使用 `render_avsync.rs`
- ~~render_nvdec.rs~~ → 使用 `AutoDecoder`
- ~~render_vaapi.rs~~ → 使用 `AutoDecoder`
- ~~test_decode_video.rs~~ → 使用 `render_avsync.rs`
- ~~test_nvdec.rs~~ → 使用 `test_auto_decoder.rs`
- ~~test_vaapi.rs~~ → 使用 `test_auto_decoder.rs`

---

**最后更新**: 2025-12-16  
**维护者**: Saide 开发团队
