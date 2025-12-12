
### 4. 音视频同时启用时音频数据稀疏问题（2025-12-12）
**现象**：
- 画面正常持续更新
- 音频线程启动并读取 2 个包后阻塞
- 通知声音可以转发，音乐播放器/媒体音无输出
- test_audio（纯音频模式）工作正常

**已排除的原因**：
1. ✅ 连接建立：video → audio → control 三个 socket 都正确建立
2. ✅ FD 传递：reverse 模式下不需要通过 control 传递 FD
3. ✅ 音频源配置：`output` 和 `playback` 模式都有相同问题
4. ✅ control 通道：启用/禁用 control 都不影响音频

**观察到的异常**：
- `adb logcat` 显示 `AudioRecord: stop()` — 音频录制被停止
- 音频包只有配置包（2个），无持续数据流

**可能原因**（待验证）：
1. **音频捕获策略**：REMOTE_SUBMIX 和 AudioPlaybackCapture 可能只捕获特定类型音频
   - 系统音（通知、铃声）✓
   - 媒体播放器音频 ✗ (可能需要特殊路由或权限)
2. **Android 权限限制**：AudioPlaybackCapture 需要应用在前台或特殊权限
3. **音频路由问题**：媒体音可能不经过 REMOTE_SUBMIX 虚拟设备

**需要的进一步测试**：
- 不同音频源场景对比（铃声 vs 音乐 vs YouTube）
- 检查 scrcpy 官方客户端是否有相同问题
- 尝试其他音频源（mic、voice_call等）
