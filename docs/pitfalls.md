
### 4. 音视频同步时音频流阻塞问题（2025-12-12）✅ 已解决

**现象**：
- 画面正常持续更新
- 音频线程启动后只读取 2 个包就永久阻塞
- 通知声音可以转发，但音乐播放器/媒体音无输出
- test_audio（纯音频模式）工作正常

**根因**：
**未读取音频流的 codec_id 导致协议错位**

scrcpy 协议规定每个流的第一个数据必须是 4 字节 codec_id：
- 视频流：`codec_id(4 bytes) + width(4) + height(4) + packets...`
- 音频流：`codec_id(4 bytes) + packets...`

我们的代码只读取了视频流的 codec metadata，完全跳过了音频流的 codec_id 读取。
导致音频线程尝试读取 12 字节包头（PTS 8 bytes + size 4 bytes）时，实际读到的是：
```
[codec_id: 4 bytes][packet_header 前 8 bytes]
```
协议完全错位，后续所有读取都失败。

**错误代码示例**：
```rust
// ❌ 错误：只读取视频流 codec metadata
let video_resolution = if params.send_codec_meta {
    stream.read_exact(&mut codec_meta[12])?; // 读取 codec_id + width + height
    ...
};

// ❌ 音频流直接跳过，没有读取 codec_id
let audio_stream = conn.audio_stream.take()?;
// 直接开始读取包头 → 错误！第一个 4 字节是 codec_id，不是包头

// 音频线程阻塞在这里
audio_stream.read_exact(&mut header[12])?; // 实际读到 codec_id + 部分包头
```

**正确修复**：
```rust
// ✅ 正确：音频流也必须先读取 codec_id
if params.send_codec_meta && let Some(ref mut stream) = audio_stream {
    let mut codec_id_bytes = [0u8; 4];
    stream.read_exact(&mut codec_id_bytes)?;
    let codec_id = u32::from_be_bytes(codec_id_bytes);
    debug!("Audio codec meta: id=0x{:08x}", codec_id); // 0x6f707573 = "opus"
    
    // 检查特殊值（参考 demuxer.c）
    if codec_id == 0 { /* 流被设备禁用 */ }
    if codec_id == 1 { /* 流配置错误 */ }
}
// 现在可以正常读取包头了
```

**诊断过程**：
1. ✅ 排除连接问题：video → audio → control 三个 socket 都正确建立
2. ✅ 排除 FD 传递：reverse 模式下不需要通过 control 传递 FD
3. ✅ 排除音频源配置：`output` 和 `playback` 模式都有相同问题
4. ✅ 对比 scrcpy 官方客户端：音视频正常工作
5. 🔍 **深度源码分析**：发现 `demuxer.c` 中 `run_demuxer()` 第一步就是读取 codec_id
6. 🎯 **定位根因**：我们的 `connection.rs` 只处理了视频流 codec metadata

**测试结果**（修复后）：
```
✅ 音频 codec 初始化：id=0x6f707573 (opus)
✅ 音频包统计：734 packets/15s 持续解码
✅ 音频处理进度：100 → 200 → ... → 700 packets
✅ 视频帧数：600+ 帧，0 丢帧
✅ 音视频同步流畅，无延迟
```

**关键教训**：
1. **协议必须完整实现**：即使某些字段看起来"可选"，也必须按协议顺序读取
2. **对比官方实现**：遇到问题时直接对比官方客户端行为，快速定位差异
3. **深度读源码**：关键协议细节往往隐藏在源码注释和边界条件处理中
4. **多流处理的对称性**：如果视频流需要读 codec_id，音频流也一定需要

**相关代码位置**：
- 修复：`src/scrcpy/connection.rs` line 185-203
- 参考：`3rd-party/scrcpy/app/src/demuxer.c` line 145-158 (`run_demuxer`)
- 协议定义：`demuxer.c` line 81-100（包头格式注释）

