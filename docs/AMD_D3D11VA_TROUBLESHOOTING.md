# AMD GPU D3D11VA Troubleshooting Quick Reference
# AMD GPU D3D11VA 故障排除快速参考

> **Status / 状态**: ✅ **Fixed in commit d7f0b25** (2026-01-27)  
> **Previous Issue / 之前问题**: Hardcoded hw_config index caused initialization failures  
> **Current Behavior / 当前表现**: D3D11VA now works on most AMD GPUs with proper drivers

---

## Problem / 问题 (Historical / 历史记录)
Windows 上运行 SAide 旧版本时出现以下错误:
```
[h264 @ ...] Failed setup for format d3d11: hwaccel initialisation returned error.
[h264 @ ...] decode_slice_header error
[h264 @ ...] no frame!
```

**Note / 注意**: If you're running **commit d7f0b25 or later**, this issue is likely fixed. See "New Diagnostics" section below.

## Affected Hardware / 受影响硬件

### Confirmed Problematic / 已确认问题硬件
- **AMD Ryzen 5 3500U (Vega 8 iGPU)** - 需驱动 21.x+ 或 BIOS UMA ≥2GB
- **AMD Ryzen 3 3200U (Vega 3 iGPU)** - 同上
- **AMD Ryzen 7 2700U (Vega 10 iGPU)** - 同上
- **Older Ryzen APUs (2017-2019)** - 驱动支持不完整

### Usually Works / 通常正常
- AMD Radeon RX 5000+ 系列 (独立显卡)
- AMD Ryzen 5000+ 系列 APU (Zen 3 + RDNA2)
- AMD Ryzen 4000 系列 APU (Zen 2 + Vega 改进版)

## Root Cause / 根本原因

### Fixed Issues / 已修复问题 (✅ Commit d7f0b25)
1. **Hardcoded hw_config index / 硬编码配置索引**: 
   - Old code assumed D3D11VA at index 0
   - Different FFmpeg builds may place it at index 1/2/etc
   - **Fix**: Now iterates all configs until D3D11VA is found

### Remaining Causes (Driver/Hardware) / 仍可能的原因 (驱动/硬件)
1. **驱动版本过旧**: Vega iGPU 的 D3D11VA 支持在 **驱动 19.x-20.x 时期不稳定**
2. **UMA 显存不足**: BIOS 默认只分配 512MB,D3D11VA 需要 ≥1GB
3. **FFmpeg 编译选项**: 部分 FFmpeg 构建未启用 D3D11VA 支持

## Quick Fix / 快速修复

### Option 1: Update Drivers (Recommended) / 方案 1: 更新驱动 (推荐)

**For Ryzen 3500U (Vega 8) users / Ryzen 3500U 用户**:
- **Minimum / 最低要求**: AMD 驱动 21.6.1 (2021-06)
- **Recommended / 推荐版本**: AMD Adrenalin 23.11.1 或更高

1. 访问 AMD 官网 / Visit AMD Support:
   - Ryzen 5 3500U 专用: https://www.amd.com/en/support/apu/amd-ryzen-processors/amd-ryzen-5-mobile-processors-radeon-vega-graphics/amd-ryzen-5-0
   - 通用页面: https://www.amd.com/en/support

2. **完全卸载旧驱动** / Uninstall old driver completely:
   ```
   设置 → 应用 → AMD Software → 卸载
   Settings → Apps → AMD Software → Uninstall
   ```

3. 下载并安装最新驱动 / Download and install latest driver:
   - 选择 **Adrenalin Edition** (不是 Minimal Setup)
   - Windows 10/11 64-bit

4. **重启两次** / Restart twice:
   - 第一次: 卸载后重启 / After uninstall
   - 第二次: 安装后重启 / After install

5. 验证安装 / Verify installation:
   ```powershell
   Get-WmiObject Win32_VideoController | Select-Object Name, DriverVersion, DriverDate
   # DriverDate 应 ≥ 2021-06-01
   ```

### Option 1.5: Adjust BIOS UMA Settings / 方案 1.5: 调整 BIOS UMA 显存

**Critical for Vega 8 APU / Vega 8 APU 必须执行**:

1. 重启电脑,进入 BIOS / Restart and enter BIOS:
   - 通常按 F2, Del, 或 F10
   - Usually press F2, Del, or F10

2. 查找 UMA 设置 / Find UMA settings:
   - `Advanced` → `UMA Frame Buffer Size`
   - `Chipset` → `Integrated Graphics Configuration`
   - `NB Configuration` → `iGPU Memory`

3. 设置为 **2GB** (或 2048MB):
   - Auto 可能只分配 512MB (不足)
   - Auto may only allocate 512MB (insufficient)

4. 保存并退出 / Save and Exit (F10)

5. 启动 Windows 验证 / Boot Windows and verify:
   ```powershell
   # 检查可用显存 / Check available VRAM
   Get-WmiObject Win32_VideoController | Select-Object AdapterRAM
   # 应显示 ≥ 2GB
   ```

### Option 2: Disable Hardware Decoding / 方案 2: 禁用硬件解码

1. 打开配置文件 / Open config file:
   ```
   %APPDATA%\saide\config.toml
   ```

2. 修改以下配置 / Change the following:
   ```toml
   [scrcpy.video]
   hwdecode = false  # Disable hardware decoding / 禁用硬件解码
   ```

3. 保存并重启 SAide / Save and restart SAide

### Option 3: Test Compatibility First / 方案 3: 先测试兼容性

运行诊断脚本 / Run diagnostic script:
```powershell
.\scripts\test_d3d11va_amd.ps1
```

根据输出结果决定修复方案 / Choose fix based on test results.

---

## New Diagnostics (Commit d7f0b25+) / 新增诊断功能 (d7f0b25+)

### Enable Debug Logging / 启用调试日志
```powershell
$env:RUST_LOG="debug"
.\target\release\saide.exe 2>&1 | Tee-Object -FilePath d3d11va_debug.log
```

### Check Hardware Config Detection / 检查硬件配置检测
```powershell
Select-String "Enumerating available hardware configs" d3d11va_debug.log -Context 0,10
```

**Expected output / 预期输出** (if fixed / 如果已修复):
```
DEBUG D3D11VA: Enumerating available hardware configs for H.264 codec
DEBUG D3D11VA: Found hw_config[0]: device_type=VAAPI, pix_fmt=..., methods=0x...
DEBUG D3D11VA: Found hw_config[1]: device_type=D3D11VA, pix_fmt=..., methods=0x...
INFO  D3D11VA: Found D3D11VA config at index 1
DEBUG D3D11VA: hardware support verified (found at config index 1)
```

**Old behavior / 旧版本行为** (before d7f0b25 / d7f0b25 之前):
```
ERROR D3D11VA: Hardware config mismatch (expected D3D11VA, got VAAPI)
```

### Check FFmpeg Error Details / 检查 FFmpeg 错误详情
```powershell
Select-String "FFmpeg error:" d3d11va_debug.log
```

**Example output / 输出示例**:
```
WARN D3D11VA: Invalid D3D11 device (EINVAL). FFmpeg error: Cannot create D3D11 video device
     Action: Update GPU drivers to latest version.
```

---

## Expected Behavior After Fix / 修复后预期行为

### Success (Hardware Decoding) / 成功 (硬件解码)
```
INFO  saide::decoder::d3d11va > D3D11VA device context created successfully
DEBUG saide::decoder::d3d11va > Enumerating available hardware configs for H.264 codec
INFO  saide::decoder::d3d11va > Found D3D11VA config at index 1
DEBUG saide::decoder::d3d11va > D3D11VA hardware support verified (found at config index 1)
INFO  saide::decoder::auto     > ✅ Using D3D11VA hardware decoder
```

### Fallback (Software Decoding) / 降级 (软件解码)
```
WARN  saide::decoder::auto > D3D11VA unavailable, falling back to software decoder
INFO  saide::decoder::auto > Using software H.264 decoder
```

**Note**: 软件解码会增加 CPU 占用,但画面质量相同。  
**Note**: Software decoding increases CPU usage, but quality is identical.

---

**Q: 我的 GPU 支持列表在哪? / Where can I find supported GPU list?**  
A: AMD UVD/VCN 支持列表:
   - UVD 6.3+: Radeon RX 400/500 系列及更新
   - VCN 1.0+: Radeon RX Vega/5000/6000 系列
   - 完整列表: https://en.wikipedia.org/wiki/Unified_Video_Decoder

**Q: 我应该使用硬件解码还是软件解码? / Should I use hardware or software decoding?**  
A: 
- 硬件解码 (hwdecode=true): 低 CPU 占用,但需驱动支持
- 软件解码 (hwdecode=false): 兼容性好,但 CPU 占用高 (5-15%)

**Q: 如何确认当前使用的解码器? / How to check current decoder?**  
A: 查看日志输出 / Check log output:
```bash
cargo run 2>&1 | Select-String "Using.*decoder"
```

## Contact / 联系方式

如问题仍未解决,请提交 Issue:  
If the issue persists, please file an issue:

https://github.com/yourusername/saide/issues

请附上以下信息 / Please include:
- GPU 型号 / GPU model (e.g., Radeon RX 580)
- 驱动版本 / Driver version
- 日志文件 / Log file: `d3d11va_test.log`
