# Scrcpy 协议分析发现

## 问题：send_codec_meta=false 参数无效

### 现象
设置 `send_codec_meta=false` 后，scrcpy-server 仍然发送 12 字节 codec meta header。

### 分析过程

1. **源码验证**：
   - `Options.java:81`: `private boolean sendCodecMeta = true;` (默认 TRUE)
   - `Streamer.java:47-56`: `if (sendCodecMeta)` 确实有检查逻辑
   - `SurfaceEncoder.java:81`: 调用 `streamer.writeVideoHeader(size)`

2. **参数传递**：
   - 我们的代码正确生成 `send_codec_meta=false` 参数
   - 通过 `adb shell CLASSPATH=... app_process ... send_codec_meta=false` 传递

3. **实际行为**：
   - Device meta 读取成功（V2507A）
   - 紧接着收到 12 字节：`68 32 36 34 00 00 02 d0 00 00 06 40`
     * `68 32 36 34` = "h264" (codec ID)
     * `00 00 02 d0` = 720 (height)
     * `00 00 06 40` = 1600 (width)

### 可能原因

1. **JAR 版本不匹配**：scrcpy-server-v3.3.3 可能是旧版本，不支持该参数
2. **参数传递失败**：ADB shell 环境变量/参数解析问题
3. **Server 实现 Bug**：某些条件下忽略该参数

### 解决方案

**采用 scrcpy 默认行为**：
- 设置 `send_codec_meta: true`（匹配官方默认值）
- 在连接建立后**始终读取**并处理 codec meta
- 这样既兼容当前行为，也符合协议规范

### 结论

**不要试图禁用 codec meta**，而是正确处理它：
1. Device meta: 64 bytes (if `send_device_meta=true`)
2. Codec meta: 12 bytes (if `send_codec_meta=true`) ← 总是读取
3. Frame packets: variable

这确保了与所有版本 scrcpy-server 的兼容性。
