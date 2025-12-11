# Codec Options 自动检测指南

## 问题背景

不同 Android 设备对 `video_codec_options` 的支持差异巨大：

| 设备示例 | SoC | Android | 支持的 Options |
|---------|-----|---------|---------------|
| vivo V2507A | MTK mt6991 | 16 | ✅ 全部支持 |
| HUAWEI ELE-AL00 | Kirin 980 | 10 | ❌ 无任何支持 |

不兼容的选项会导致 `MediaCodec$CodecException: Error 0x80001001` 崩溃。

---

## 自动检测流程

### 1. 运行探测工具

```bash
# 自动检测当前连接的设备
cargo run --example probe_codec

# 或指定设备序列号
cargo run --example probe_codec 10AF971ZLN004SU
```

### 2. 探测过程

工具会逐个测试以下选项（按延迟影响排序）：

```
Testing codec options...
  [1/8] Testing profile=66...           ❌ Not supported
  [2/8] Testing i-frame-interval=2...   ✅ Supported
  [3/8] Testing latency=0...            ✅ Supported
  [4/8] Testing max-bframes=0...        ❌ Not supported (Android 13+)
  [5/8] Testing priority=0...           ✅ Supported
  [6/8] Testing prepend-sps-pps-to-idr-frames=1... ✅ Supported
  [7/8] Testing intra-refresh-period=60... ✅ Supported
  [8/8] Testing bitrate-mode=1...       ✅ Supported
```

### 3. 配置缓存

结果保存到 `~/.config/saide/device_profiles.json`：

```json
{
  "profiles": {
    "10AF971ZLN004SU": {
      "serial": "10AF971ZLN004SU",
      "model": "V2507A",
      "platform": "mt6991",
      "android_version": 36,
      "supported_options": [
        "i-frame-interval",
        "latency",
        "priority",
        "prepend-sps-pps-to-idr-frames",
        "intra-refresh-period",
        "bitrate-mode"
      ],
      "optimal_config": "i-frame-interval=2,latency=0,priority=0,prepend-sps-pps-to-idr-frames=1,intra-refresh-period=60,bitrate-mode=1",
      "tested_at": "2025-12-11T15:00:00Z"
    }
  }
}
```

---

## 使用方法

### 方式 1：自动加载（推荐）

```rust
use saide::{ScrcpyConnection, ServerParams};

// 自动从缓存加载设备配置
let params = ServerParams::for_device(&serial)?;

let conn = ScrcpyConnection::connect(&serial, server_jar, params).await?;
```

### 方式 2：手动指定

```rust
let params = ServerParams {
    video_codec_options: Some("i-frame-interval=2,bitrate-mode=1".to_string()),
    ..Default::default()
};
```

### 方式 3：完全禁用

```rust
let params = ServerParams {
    video_codec_options: None, // 使用编码器默认配置
    ..Default::default()
};
```

---

## 各选项说明

| 选项 | 值 | 延迟改善 | 最低 Android | 说明 |
|------|---|---------|--------------|------|
| `profile` | 66 | ⭐⭐⭐ | 4.1 | Baseline Profile（无B帧，部分设备不支持） |
| `i-frame-interval` | 2 | ⭐⭐⭐ | 4.3 | 2秒GOP，减少关键帧等待 |
| `latency` | 0 | ⭐⭐ | 11 | 最低延迟模式 |
| `max-bframes` | 0 | ⭐⭐ | 13 | 禁用B帧（与 profile=66 冲突） |
| `priority` | 0 | ⭐ | 5.0 | 实时编码优先级 |
| `prepend-sps-pps-to-idr-frames` | 1 | - | 4.3 | 支持动态分辨率 |
| `intra-refresh-period` | 60 | - | 4.4 | 周期性帧内刷新 |
| `bitrate-mode` | 1 | - | 4.3 | CBR 固定码率 |

---

## 故障排查

### Q: 探测显示全部不支持？

**A**: 正常现象（如 Kirin 980），使用 `video_codec_options: None` 即可。

### Q: 连接时仍然崩溃？

**A**: 手动运行探测：
```bash
cargo run --example probe_codec
```

然后检查 `~/.config/saide/device_profiles.json` 是否正确。

### Q: 想重新探测设备？

**A**: 删除配置文件后重新运行：
```bash
rm ~/.config/saide/device_profiles.json
cargo run --example probe_codec
```

---

## 已知兼容性

### ✅ 完全支持（6+ options）
- MTK mt6991 (Android 16)
- Qualcomm SM8550 (Android 14)
- Exynos 2400 (Android 14)

### ⚠️ 部分支持（2-5 options）
- MTK mt6891 (Android 13)
- Qualcomm SM8350 (Android 12)

### ❌ 无支持
- Kirin 980 (Android 10)
- Exynos 9820 (Android 10)

**贡献你的设备数据**：欢迎提交 issue 附带探测结果！
