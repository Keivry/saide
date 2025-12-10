# Scrcpy 协议实现测试

本目录包含两个测试示例，用于验证 Scrcpy 协议实现的正确性。

## 测试 1：协议验证（无需设备）

测试控制协议和视频协议的序列化/反序列化功能。

```bash
cargo run --example test_protocol
```

**测试内容**：
- ✅ 控制消息序列化（9 种消息类型）
- ✅ 视频包解析（CONFIG/KEYFRAME/P-frame）
- ✅ 二进制格式验证

**预期输出**：
```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
🧪 Scrcpy 协议实现验证测试
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

📋 测试 1: 控制协议序列化
  ✓ Touch Down: 32 字节
  ✓ Touch Move: 32 字节
  ...
  ✅ 控制协议测试通过 (9 种消息)

📹 测试 2: 视频协议解析
  ✓ Config Packet (SPS/PPS)
  ✓ Keyframe (IDR)
  ✓ P-frame
  ✅ 视频协议测试通过 (3 个包)

✅ 所有协议测试通过!
```

---

## 测试 2：真实设备连接（需要设备）

测试与真实 Android 设备的完整连接流程。

### 前提条件

1. **Android 设备已连接**
   ```bash
   adb devices
   ```
   应显示至少一个设备。

2. **启用 USB 调试**
   - 设置 → 开发者选项 → USB 调试

3. **Server JAR 存在**
   ```bash
   ls 3rd-party/scrcpy-server-v3.3.3
   ```

### 运行测试

```bash
# 自动使用第一个可用设备
cargo run --example test_connection

# 或指定设备序列号
cargo run --example test_connection 10AF971ZLN004SU
```

**测试内容**：
1. ✅ Server 推送与启动
2. ✅ ADB reverse 隧道建立
3. ✅ 三路 socket 连接（Video/Control）
4. ✅ 视频流接收（10 个数据包）
5. ✅ 控制消息发送（3 条）
6. ✅ Server 进程监控
7. ✅ 优雅关闭

**预期输出**：
```
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
🧪 Scrcpy 协议实现测试
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
📱 设备: 10AF971ZLN004SU
✓ Server JAR: 3rd-party/scrcpy-server-v3.3.3

📋 配置:
  SCID: 1a2b3c4d
  视频: h264 @ 8000000bps
  分辨率: 1600px, 帧率: 60fps

🔌 建立连接中...
✅ 连接成功!
  本地端口: 27183

📹 测试 1: 读取视频流
  读取 10 个视频包...
  [ 1] CONFIG    245 bytes (SPS/PPS)
  [ 2] KEYFRAME  15234 bytes, PTS=0μs
  [ 3] P-FRAME   3421 bytes, PTS=16667μs
  ...
  统计: 总计=10, CONFIG=1, 关键帧=2, P帧=7

🎮 测试 2: 发送控制消息
  ✓ 折叠通知栏 (1 字节)
  ✓ 触摸按下 (32 字节)
  ✓ 触摸抬起 (32 字节)
  ✅ 发送 3 条消息

⚙️  测试 3: 服务器状态
✅ Server 进程正常运行

🛑 关闭连接...

━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
✅ 所有测试通过!
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
```

### 常见问题

**Q: `未找到 Android 设备`**
```bash
# 检查设备连接
adb devices

# 重启 adb 服务
adb kill-server
adb start-server
```

**Q: `Server JAR 不存在`**
```bash
# 确认文件存在
ls 3rd-party/scrcpy-server-v3.3.3

# 如果不存在，从 scrcpy 官方下载
wget https://github.com/Genymobile/scrcpy/releases/download/v3.3.3/scrcpy-server-v3.3.3
mv scrcpy-server-v3.3.3 3rd-party/
```

**Q: `Failed to push server`**
```bash
# 检查设备存储空间
adb shell df /data/local/tmp

# 手动清理旧 server
adb shell rm /data/local/tmp/scrcpy-server.jar
```

**Q: `Connection refused`**
- 检查防火墙设置
- 确认端口 27183-27199 可用
- 尝试重新连接设备

---

## 调试模式

启用详细日志输出：

```bash
RUST_LOG=debug cargo run --example test_connection
```

查看 ADB 日志：

```bash
adb logcat | grep scrcpy
```

---

## 验证结果

如果两个测试都通过，说明：

✅ **协议实现正确**：控制消息和视频包格式符合 scrcpy 规范  
✅ **连接流程正常**：Server 启动、端口转发、Socket 握手成功  
✅ **数据传输稳定**：视频流和控制流双向通信正常  
✅ **进程管理健壮**：资源清理、异常处理完善  

可以进入下一阶段：集成 FFmpeg 解码器。
