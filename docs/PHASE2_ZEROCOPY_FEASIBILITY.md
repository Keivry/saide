# Phase 2: 零拷贝 GPU 解码可行性分析

> **创建时间**: 2026-01-15  
> **最终状态**: ❌ **已终止 - wgpu v28 不支持外部内存导入**  
> **原因**: wgpu 28.0.0 未暴露 hal 公开 API，无法实现零拷贝  
> **替代方案**: 执行 Phase 3 (音频 + 输入优化，预期 8-18ms 降低)

---

## 目录

- [当前架构](#当前架构)
- [技术方案](#技术方案)
  - [NVDEC 零拷贝路径](#nvdec-零拷贝路径)
  - [VAAPI 零拷贝路径](#vaapi-零拷贝路径)
- [技术挑战](#技术挑战)
- [实施计划](#实施计划)
- [风险评估](#风险评估)

---

## 当前架构

### 现有视频管道 (Phase 1)

```
┌─────────────┐
│ H264 Packet │
└──────┬──────┘
       │
       v
┌──────────────────┐
│ FFmpeg Decoder   │
│ (NVDEC/VAAPI)   │ ← 硬件解码器输出 AV_PIX_FMT_CUDA/VAAPI
└──────┬───────────┘
       │
       v
┌──────────────────────────┐
│ av_hwframe_transfer_data │ ← ⚠️ GPU → CPU 拷贝 (10-15ms)
└──────┬───────────────────┘
       │
       v
┌──────────────────┐
│ NV12 CPU Buffer  │
└──────┬───────────┘
       │
       v
┌──────────────────┐
│ queue.write_texture │ ← ⚠️ CPU → GPU 拷贝 (2-5ms)
│ (Y plane + UV)    │
└──────┬────────────┘
       │
       v
┌──────────────────┐
│ GPU Shader       │ ← YUV→RGBA 转换 (GPU 端)
│ (nv12_shader.wgsl)│
└──────────────────┘
```

**当前瓶颈**:
1. **GPU → CPU 拷贝**: `av_hwframe_transfer_data()` - 10-15ms
2. **CPU → GPU 上传**: `queue.write_texture()` - 2-5ms
3. **总延迟**: 12-20ms (可优化)

**关键代码位置**:
- NVDEC: `src/decoder/nvdec.rs:150`
- VAAPI: `src/decoder/vaapi.rs:189`
- GPU 上传: `src/decoder/nv12_render.rs:289-326`

---

## 技术方案

### NVDEC 零拷贝路径

#### 目标架构

```
┌─────────────┐
│ H264 Packet │
└──────┬──────┘
       │
       v
┌──────────────────┐
│ NVDEC Decoder    │
│ Output: CUDA     │ ← AV_PIX_FMT_CUDA (保持在 GPU)
└──────┬───────────┘
       │
       v
┌──────────────────────┐
│ CUDA Texture Export  │ ← 导出 CUdeviceptr
└──────┬───────────────┘
       │
       v
┌──────────────────────┐
│ WGPU Vulkan External │ ← vkImportMemoryFdKHR
│ Memory Import        │
└──────┬───────────────┘
       │
       v
┌──────────────────┐
│ GPU Shader       │ ← YUV→RGBA (已存在)
│ (nv12_shader.wgsl)│
└──────────────────┘
```

#### 技术路径

**1. FFmpeg 侧修改**

```rust
// 位置: src/decoder/nvdec.rs

// 当前 (Phase 1):
unsafe {
    (*ctx_ptr).sw_pix_fmt = ffmpeg::sys::AVPixelFormat::AV_PIX_FMT_NV12;
}

// Phase 2 优化:
unsafe {
    // 不设置 sw_pix_fmt,强制保持 CUDA 格式
    // 或设置为 AV_PIX_FMT_CUDA (但 FFmpeg 会尝试自动转换)
}
```

**2. CUDA → Vulkan 互操作**

```rust
// 新建: src/decoder/cuda_interop.rs

use std::ffi::c_void;

/// CUDA device pointer (from FFmpeg AVFrame)
pub struct CudaTextureHandle {
    ptr: *mut c_void,
    pitch: usize,
    width: u32,
    height: u32,
}

impl CudaTextureHandle {
    /// 从 FFmpeg AVFrame 提取 CUDA 纹理
    pub unsafe fn from_avframe(frame: *mut ffmpeg::sys::AVFrame) -> Result<Self> {
        // 1. 检查格式是否为 AV_PIX_FMT_CUDA
        let format = (*frame).format;
        if format != ffmpeg::sys::AVPixelFormat::AV_PIX_FMT_CUDA as i32 {
            return Err(Error::InvalidFormat);
        }

        // 2. 获取 CUDA device pointer
        let data_ptr = (*frame).data[0] as *mut c_void;
        let pitch = (*frame).linesize[0] as usize;

        Ok(CudaTextureHandle {
            ptr: data_ptr,
            pitch,
            width: (*frame).width as u32,
            height: (*frame).height as u32,
        })
    }

    /// 导出为 Vulkan external memory (需要 CUDA Driver API)
    pub fn export_to_vulkan(&self, device: &wgpu::Device) -> Result<wgpu::Texture> {
        // 1. CUDA: cuMemExportToShareableHandle() (POSIX file descriptor)
        // 2. Vulkan: vkImportMemoryFdKHR()
        // 3. wgpu: create_texture_from_hal() (需要 wgpu-hal)
        todo!("Requires CUDA Driver API + wgpu-hal")
    }
}
```

**3. wgpu 集成**

```rust
// 位置: src/decoder/nv12_render.rs

impl Nv12RenderResources {
    pub fn upload_frame_zerocopy(
        &mut self,
        device: &wgpu::Device,
        frame: &DecodedFrame,
    ) {
        // Phase 2: 直接导入 CUDA 纹理
        if let Some(cuda_handle) = frame.cuda_handle {
            self.y_texture = Some(cuda_handle.export_to_vulkan(device)?);
            // UV texture 同理
        } else {
            // Fallback: 使用当前 Phase 1 路径
            self.upload_frame_cpu(device, queue, frame);
        }
    }
}
```

**技术依赖**:
- ✅ CUDA Driver API (cuMemExportToShareableHandle)
- ✅ Vulkan External Memory Extensions (VK_KHR_external_memory_fd)
- ⚠️ wgpu-hal 直接访问 (当前使用 wgpu 高层 API)
- ⚠️ FFmpeg 保持 CUDA 格式不自动转换

**预期收益**: 15-20ms (消除 GPU→CPU→GPU 往返)

---

### VAAPI 零拷贝路径

#### 目标架构

```
┌─────────────┐
│ H264 Packet │
└──────┬──────┘
       │
       v
┌──────────────────┐
│ VAAPI Decoder    │
│ Output: VAAPI    │ ← AV_PIX_FMT_VAAPI (VASurface)
└──────┬───────────┘
       │
       v
┌──────────────────────┐
│ VA Surface Export    │ ← vaExportSurfaceHandle (DMA-BUF)
└──────┬───────────────┘
       │
       v
┌──────────────────────┐
│ WGPU Vulkan External │ ← vkImportMemoryFdKHR
│ Memory Import        │
└──────┬───────────────┘
       │
       v
┌──────────────────┐
│ GPU Shader       │ ← YUV→RGBA (已存在)
│ (nv12_shader.wgsl)│
└──────────────────┘
```

#### 技术路径

**1. FFmpeg 侧修改**

```rust
// 位置: src/decoder/vaapi.rs

// 当前 (Phase 1):
unsafe {
    (*ctx_ptr).sw_pix_fmt = ffmpeg::sys::AVPixelFormat::AV_PIX_FMT_NV12;
}

// Phase 2 优化:
unsafe {
    // 不设置 sw_pix_fmt,保持 VAAPI 格式
}
```

**2. VA Surface → DMA-BUF 导出**

```rust
// 新建: src/decoder/vaapi_interop.rs

use libva_sys::*; // 需要添加依赖

pub struct VaSurfaceHandle {
    display: VADisplay,
    surface_id: VASurfaceID,
    width: u32,
    height: u32,
}

impl VaSurfaceHandle {
    /// 从 FFmpeg AVFrame 提取 VA Surface
    pub unsafe fn from_avframe(frame: *mut ffmpeg::sys::AVFrame) -> Result<Self> {
        // 1. 获取 AVHWFramesContext
        let hw_frames_ctx = (*frame).hw_frames_ctx;
        if hw_frames_ctx.is_null() {
            return Err(Error::NoHwContext);
        }

        let frames_ctx = (*hw_frames_ctx).data as *mut ffmpeg::sys::AVHWFramesContext;
        let device_ctx = (*frames_ctx).device_ctx as *mut ffmpeg::sys::AVHWDeviceContext;
        let vaapi_ctx = (*device_ctx).hwctx as *mut ffmpeg::sys::AVVAAPIDeviceContext;

        let display = (*vaapi_ctx).display;
        let surface_id = (*frame).data[3] as VASurfaceID;

        Ok(VaSurfaceHandle {
            display,
            surface_id,
            width: (*frame).width as u32,
            height: (*frame).height as u32,
        })
    }

    /// 导出为 DMA-BUF (POSIX file descriptor)
    pub fn export_dma_buf(&self) -> Result<DmaBufHandle> {
        unsafe {
            let mut descriptor = VADRMPRIMESurfaceDescriptor::default();
            let status = vaExportSurfaceHandle(
                self.display,
                self.surface_id,
                VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2,
                VA_EXPORT_SURFACE_READ_ONLY,
                &mut descriptor,
            );

            if status != VA_STATUS_SUCCESS {
                return Err(Error::VaExportFailed(status));
            }

            // descriptor.objects[0].fd 即为 DMA-BUF fd
            Ok(DmaBufHandle {
                fd: descriptor.objects[0].fd,
                width: self.width,
                height: self.height,
                format: descriptor.fourcc, // 应为 NV12
            })
        }
    }
}

pub struct DmaBufHandle {
    fd: i32, // File descriptor
    width: u32,
    height: u32,
    format: u32, // FourCC (NV12)
}

impl DmaBufHandle {
    /// 导入到 Vulkan (通过 wgpu-hal)
    pub fn import_to_vulkan(&self, device: &wgpu::Device) -> Result<wgpu::Texture> {
        // 使用 vkImportMemoryFdKHR
        todo!("Requires wgpu-hal + VK_KHR_external_memory_fd")
    }
}
```

**3. wgpu 集成**

```rust
// 位置: src/decoder/nv12_render.rs

impl Nv12RenderResources {
    pub fn upload_frame_zerocopy_vaapi(
        &mut self,
        device: &wgpu::Device,
        frame: &DecodedFrame,
    ) {
        // Phase 2: 直接导入 DMA-BUF
        if let Some(va_handle) = frame.va_surface {
            let dma_buf = va_handle.export_dma_buf()?;
            self.y_texture = Some(dma_buf.import_to_vulkan(device)?);
            // UV texture 同理
        } else {
            // Fallback: Phase 1 CPU 路径
            self.upload_frame_cpu(device, queue, frame);
        }
    }
}
```

**技术依赖**:
- ✅ libva 2.0+ (vaExportSurfaceHandle)
- ✅ Vulkan External Memory Extensions (VK_KHR_external_memory_fd)
- ⚠️ wgpu-hal 直接访问
- ✅ DRM/KMS (Linux only)

**预期收益**: 15-20ms

---

## 技术挑战

### 1. wgpu 高层 API 限制 ⚠️

**问题**: wgpu 0.27 高层 API 不支持外部内存导入

**当前依赖**:
```toml
wgpu = { version = "27", features = ["vulkan"] }
```

**解决方案**:

#### 选项 A: 使用 wgpu-hal (推荐)

```rust
// 添加依赖
use wgpu_hal as hal;

// 从 wgpu::Device 获取底层 Vulkan device
let hal_device = device.as_hal::<hal::api::Vulkan, _, _>(|device| {
    // 直接调用 Vulkan API
    unsafe {
        device.raw_device().import_memory_fd(...)
    }
});
```

**优点**: 
- 保持 wgpu 统一渲染管线
- 仅在纹理导入时使用底层 API

**缺点**:
- wgpu-hal API 不稳定 (可能每版本变化)
- 需要 unsafe 代码

#### 选项 B: 切换到 ash (Vulkan 直接绑定)

```toml
ash = { version = "0.39", features = ["linked"] }
```

**优点**:
- 完全控制 Vulkan 资源
- 稳定 API

**缺点**:
- 需要重写整个渲染管线 (nv12_render.rs 300+ 行)
- 与 eframe/egui 集成复杂

**推荐**: 选项 A (wgpu-hal)

---

### 2. FFmpeg 格式协商 ⚠️

**问题**: FFmpeg 会自动调用 `av_hwframe_transfer_data` 如果检测到软件格式需求

**当前代码**:
```rust
// src/decoder/nvdec.rs:91
(*ctx_ptr).sw_pix_fmt = ffmpeg::sys::AVPixelFormat::AV_PIX_FMT_NV12;
```

**这会导致 FFmpeg 认为我们需要 CPU 端 NV12,从而自动触发 GPU→CPU 拷贝**

**解决方案**:

#### 选项 A: 不设置 sw_pix_fmt (推荐)

```rust
// Phase 2: 删除 sw_pix_fmt 设置
// (*ctx_ptr).sw_pix_fmt = ...; // 删除这行
```

**风险**: 需要验证解码器不会 fallback 到软件解码

#### 选项 B: 手动处理 AVFrame

```rust
// 解码后检查格式
let frame = decoder.receive_frame()?;
if frame.format() == Pixel::CUDA {
    // 保持 CUDA 格式,不调用 transfer_data
    return Ok(Some(frame));
} else {
    // Fallback 到当前路径
}
```

---

### 3. 跨 API 同步 (CUDA ↔ Vulkan / VAAPI ↔ Vulkan)

**问题**: GPU 内存在不同 API 间共享需要显式同步

**CUDA → Vulkan**:
- 需要 `cuStreamSynchronize()` 确保 CUDA 解码完成
- Vulkan 需要 Semaphore 等待

**VAAPI → Vulkan**:
- DMA-BUF 本身支持隐式同步 (DMA fence)
- 但需要 Vulkan Implicit Sync 扩展 (VK_KHR_external_semaphore_fd)

**解决方案**:

```rust
// CUDA 侧
unsafe {
    cuStreamSynchronize(stream); // 确保解码完成
}

// Vulkan 侧 (wgpu-hal)
device.wait_for_cuda_fence(...);
```

**风险**: 错误同步可能导致花屏或崩溃

---

### 4. 纹理格式兼容性

**当前着色器** (`nv12_shader.wgsl`):
- Y 平面: `R8Unorm` (单通道灰度)
- UV 平面: `Rg8Unorm` (双通道交错)

**零拷贝路径**:
- CUDA/VAAPI 导出的纹理格式可能为 `NV12` 专用格式
- 需要验证 Vulkan 是否支持 `VK_FORMAT_G8_B8R8_2PLANE_420_UNORM`

**解决方案**:

#### 选项 A: 使用 Vulkan YCbCr Sampler

```rust
// 创建 YCbCr sampler (原生支持 NV12)
let sampler = device.create_sampler_ycbcr(...);
```

**优点**: 硬件原生支持,性能最优
**缺点**: wgpu 0.27 不支持 (需要 wgpu-hal)

#### 选项 B: 保持当前双纹理方案

```rust
// 导入时分离 Y 和 UV 平面 (DMA-BUF 支持多 plane)
let y_texture = import_plane(dma_buf, plane_index = 0);
let uv_texture = import_plane(dma_buf, plane_index = 1);
```

**推荐**: 选项 B (兼容性更好)

---

## 实施计划

### 阶段 2.1: wgpu-hal 原型验证 (1-2 天)

**目标**: 验证 wgpu-hal 能否导入外部内存

**任务**:
1. 创建最小测试用例 (`examples/test_vulkan_import.rs`)
2. 使用 wgpu-hal 手动导入 DMA-BUF (模拟数据)
3. 验证纹理能否在 wgpu Pipeline 中使用

**成功标准**: 能够显示从 DMA-BUF 导入的纹理

**风险退出**: 如果 wgpu-hal API 不稳定或不支持 → 推迟到 wgpu 0.28+

---

### 阶段 2.2: VAAPI DMA-BUF 导出 (2-3 天)

**目标**: 实现 VAAPI → DMA-BUF 导出

**任务**:
1. 添加 `libva` 依赖 (`Cargo.toml`)
2. 实现 `VaSurfaceHandle::export_dma_buf()`
3. 修改 `vaapi.rs` 不设置 `sw_pix_fmt`
4. 测试解码器输出格式为 `AV_PIX_FMT_VAAPI`

**测试**:
- 使用 `examples/probe_codec.rs` 验证解码器格式
- 打印 DMA-BUF fd 和格式信息

**风险**:
- VAAPI 驱动版本兼容性 (需要 Mesa 22.0+)
- 某些旧 GPU 不支持 DMA-BUF 导出

---

### 阶段 2.3: Vulkan 纹理导入 (3-4 天)

**目标**: DMA-BUF → wgpu Texture

**任务**:
1. 实现 `DmaBufHandle::import_to_vulkan()` (使用 wgpu-hal)
2. 处理多平面 NV12 格式 (Y + UV 分离导入)
3. 集成到 `Nv12RenderResources::upload_frame_zerocopy()`

**测试**:
- 修改 `examples/test_connection.rs` 使用零拷贝路径
- 对比 Phase 1 和 Phase 2 延迟数据

**风险**:
- Vulkan 扩展支持 (VK_KHR_external_memory_fd)
- GPU 驱动 bug (需要多硬件测试)

---

### 阶段 2.4: NVDEC CUDA 互操作 (3-5 天) - 可选

**目标**: CUDA → Vulkan 零拷贝 (仅 NVIDIA GPU)

**任务**:
1. 添加 CUDA Driver API 绑定 (或使用 `cuda-sys` crate)
2. 实现 `CudaTextureHandle::export_to_vulkan()`
3. 处理 CUDA-Vulkan 同步 (stream sync)

**测试**:
- NVIDIA GPU 真机测试
- 对比 NVDEC Phase 1 vs Phase 2 延迟

**风险**:
- CUDA Driver API 复杂性
- CUDA-Vulkan 互操作稳定性 (驱动依赖)

**优先级**: P1 (VAAPI 完成后再做,AMD/Intel 用户更多)

---

### 阶段 2.5: Fallback 机制 (1 天)

**目标**: 自动降级到 Phase 1 路径

**任务**:
1. 检测零拷贝路径是否可用 (检查 Vulkan 扩展)
2. 如果失败,fallback 到 `av_hwframe_transfer_data`
3. 日志记录降级原因

**实现**:
```rust
impl Nv12RenderResources {
    pub fn upload_frame(&mut self, device: &wgpu::Device, frame: &DecodedFrame) {
        if self.supports_zerocopy && frame.has_hw_handle() {
            match self.upload_frame_zerocopy(device, frame) {
                Ok(_) => return,
                Err(e) => {
                    warn!("Zero-copy upload failed: {e}, fallback to CPU path");
                    self.supports_zerocopy = false;
                }
            }
        }
        
        // Fallback: Phase 1 CPU 路径
        self.upload_frame_cpu(device, queue, frame);
    }
}
```

---

### 阶段 2.6: 性能验证 (1-2 天)

**目标**: 量化延迟改进

**任务**:
1. 更新 `LatencyProfiler` 支持零拷贝路径时间测量
2. 对比测试:
   - Phase 1 (CPU 路径): 预期 30-50ms
   - Phase 2 (零拷贝): 预期 15-30ms
3. 多硬件测试 (AMD/Intel/NVIDIA)

**成功标准**: 平均延迟降低 15ms+

---

## 风险评估

| 风险项 | 概率 | 影响 | 缓解措施 |
|--------|------|------|----------|
| wgpu-hal API 不稳定 | 中 | 高 | 优先原型验证,考虑锁定 wgpu 版本 |
| VAAPI 驱动兼容性 | 中 | 中 | 提供 CPU fallback,文档说明最低驱动版本 |
| Vulkan 扩展缺失 | 低 | 高 | 运行时检测,自动降级 |
| CUDA 互操作复杂 | 高 | 中 | 推迟到 VAAPI 完成后,标记为可选特性 |
| 花屏/崩溃 | 中 | 高 | 充分测试同步逻辑,添加验证层 |
| 性能提升不明显 | 低 | 高 | 预先 profiling 确认瓶颈,最坏回退 Phase 1 |

**关键依赖**:
- ✅ wgpu 0.27 + hal 功能
- ⚠️ libva 2.0+ (VAAPI)
- ⚠️ Mesa 22.0+ / NVIDIA 525+ 驱动
- ⚠️ Vulkan 1.1+ 外部内存扩展

**推荐策略**: 
1. 优先实现 VAAPI 路径 (覆盖 AMD/Intel 用户)
2. NVDEC 作为第二优先级 (仅 NVIDIA)
3. 保留 Phase 1 CPU 路径作为通用 fallback

---

## 下一步行动

### 立即行动 (优先级 P0)

1. **wgpu-hal 原型验证** (`阶段 2.1`)
   - 创建 `examples/test_vulkan_import.rs`
   - 验证 wgpu-hal API 可用性
   - 预计耗时: 1-2 天

2. **添加依赖**
   ```toml
   # Cargo.toml
   libva = "0.5"  # VAAPI bindings
   ash = { version = "0.39", optional = true }  # Vulkan fallback
   ```

3. **更新文档**
   - `docs/ARCHITECTURE.md`: 添加零拷贝流程图
   - `README.md`: 说明 Phase 2 实验性特性

### 后续规划 (优先级 P1)

4. **VAAPI 实现** (`阶段 2.2 + 2.3`)
5. **性能验证** (`阶段 2.6`)
6. **NVDEC 实现** (`阶段 2.4`) - 可选

---

## 原型验证结果 (2026-01-15)

### wgpu-hal API 可用性测试

**测试文件**: `examples/test_vulkan_import.rs`

**执行命令**:
```bash
WGPU_BACKEND=vulkan cargo run --example test_vulkan_import
```

**测试结果**:
```
=== wgpu-hal External Memory Import Prototype ===

Adapter: NVIDIA GeForce RTX 4070 Laptop GPU
Backend: Vulkan

✅ Platform: Linux (DMA-BUF supported)

❌ wgpu-hal feature not enabled
   wgpu 0.27 public API does not expose hal by default

   Investigation findings:
   - wgpu::Device::as_hal() is NOT available in public API
   - External memory import requires raw Vulkan calls
   - Alternative: Use ash crate directly (100% Vulkan control)
```

### 关键发现

1. **wgpu 0.27 不支持外部内存导入**
   - `wgpu::Device` 不提供 `as_hal()` 公开 API
   - `wgpu-hal` 模块存在但未公开暴露
   - 外部内存导入需要直接 Vulkan 调用 (ash crate)

2. **GPU 功能确认**
   - 测试 GPU: NVIDIA RTX 4070 Laptop
   - 支持特性包括: `TEXTURE_FORMAT_NV12`, `EXTERNAL_TEXTURE` (但无法通过 wgpu API 使用)
   - Vulkan backend 正常工作

3. **技术路径评估**

| 方案 | 可行性 | 工作量 | 风险 |
|-----|--------|--------|------|
| 等待 wgpu 0.28+ | ⚠️ 未知 | 低 (被动等待) | 时间不确定 |
| 使用 ash (重写渲染) | ✅ 可行 | **极高** (300+ 行) | 中 (兼容性) |
| 保持 Phase 1 CPU 路径 | ✅ 可行 | 低 (已完成) | 低 (成熟方案) |

---

## 最终结论

### Phase 2 零拷贝实施决策: **重新评估 wgpu v28.0.0** ⚠️

> **更新 2026-01-15 晚间**: wgpu v28.0.0 已于 2024-12-18 发布!  
> **行动**: 需验证 external memory API 是否在此版本中可用

**原暂缓理由** (基于 v27.0.0):

1. **技术栈限制**
   - wgpu 0.27 无公开 hal API
   - 切换到 ash 需重写整个渲染管线 (`nv12_render.rs` 333 行 + egui 集成)
   - 投入产出比不合理 (预期 20ms 收益 vs 1-2 周开发时间)

2. **Phase 1 已足够优秀**
   - 当前 CPU 路径延迟: 12-20ms (GPU→CPU→GPU)
   - Phase 1 优化已降低 13-25ms (解码器标志 + 网络 + AV 同步)
   - **总延迟目标**: 30-50ms → 20-35ms (已达成主要目标)

**wgpu v28.0.0 新特性 (待验证)**:

根据发布说明,v28.0.0 包含以下重大更新:
- ✅ Mesh Shaders 支持 (Vulkan/Metal/DX12)
- ✅ 异步 `enumerate_adapters` (现支持 WebGPU)
- ✅ 新增 `LoadOp::DontCare`
- ⚠️  **External memory API 支持待确认**

**下一步行动 (优先级 P0)**:

1. **升级依赖到 wgpu 28**
   ```toml
   wgpu = { version = "28", features = ["vulkan"] }
   ```

2. **重新运行原型测试**
   ```bash
   # 检查 v28 hal API 可用性
   cargo update -p wgpu
   WGPU_BACKEND=vulkan cargo run --example test_vulkan_import
   ```

3. **检查 changelog 和文档**
   - 查找 `external_memory`, `as_hal`, `import_texture` 等关键词
   - 验证 Vulkan external memory fd 导入 API
   - 确认 VAAPI/CUDA interop 支持

4. **If v28 支持 external memory**:
   - ✅ 解除 Phase 2 暂缓状态
   - ✅ 继续实施 VAAPI DMA-BUF 导出 (阶段 2.2)
   - ✅ 预期收益: 20-40ms 额外延迟降低

5. **If v28 仍不支持**:
   - ❌ 继续 Phase 3 替代优化 (音频 + 输入)
   - ⏳ 等待 wgpu v29+ 或提交 Feature Request

---

### 未来路径
   - 监控 wgpu 0.28+ 版本更新 (2026 Q2?)
   - 如果 wgpu 提供稳定外部内存 API,重启 Phase 2 评估
   - 或考虑上游贡献: 向 wgpu 项目提 PR 添加外部内存支持

### 替代优化方向 (Phase 3 - 可选)

由于零拷贝路径暂缓,建议转向其他低悬挂果实:

1. **音频延迟优化** (预期 3-8ms)
   - CPAL 独占模式 (详见 `docs/LATENCY_OPTIMIZATION.md` 章节 2.2)
   - 音频缓冲降至 64 frames

2. **输入延迟优化** (预期 5-10ms)
   - Linux evdev 原始输入监听 (绕过 egui 事件循环)
   - 鼠标移动速度自适应

3. **用户配置化**
   - 可配置禁用 AV 同步 (牺牲音画同步换取 5-10ms)
   - GPU 解码器优先级配置

**预期总收益**: Phase 1 (13-25ms) + Phase 3 (8-18ms) = **21-43ms** 延迟降低

---

## 测试产物

### 可运行示例

```bash
# 验证 Vulkan backend 可用性
WGPU_BACKEND=vulkan cargo run --example test_vulkan_import

# 检查系统 Vulkan 扩展
vulkaninfo | grep -i external

# 预期输出 (如果支持 DMA-BUF):
# VK_KHR_external_memory_fd : extension revision 1
# VK_KHR_external_semaphore_fd : extension revision 1
```

### 文档输出

- ✅ `docs/PHASE2_ZEROCOPY_FEASIBILITY.md` (本文档)
- ✅ `examples/test_vulkan_import.rs` (原型测试代码)
- ⏭️  `docs/PHASE3_ALTERNATIVE_OPTIMIZATIONS.md` (待创建)

---

---

## wgpu v28 验证结果 (2026-01-15)

### 测试方法

1. **手动文档检查**:
   - docs.rs/wgpu/28.0.0 - 未找到 `Device::as_hal()` 方法
   - docs.rs/wgpu-hal/28.0.0 - hal 模块虽被 re-export,但未公开给用户代码

2. **自动检索** (librarian agent):
   - 搜索关键词: external memory, DMA-BUF, `as_hal`, import texture
   - 结果: 未发现相关 API 或 changelog 条目

### 最终结论

**❌ wgpu v28.0.0 仍不支持外部内存导入**

| 检查项 | v27 状态 | v28 状态 | 说明 |
|-------|---------|---------|------|
| `Device::as_hal()` | ❌ | ❌ | 未暴露 hal 访问接口 |
| `wgpu::hal` 公开模块 | ❌ | ❌ | 仅 crate 内部 re-export |
| External memory API | ❌ | ❌ | 未发现相关文档或方法 |
| DMA-BUF import | ❌ | ❌ | Vulkan 专有扩展未桥接 |

**v28 主要新特性** (与本项目无关):
- ✅ Mesh Shaders (Vulkan/Metal/DX12)
- ✅ `LoadOp::DontCare` (减少 TBDR GPU 带宽)
- ✅ `async enumerate_adapters()`
- ⚠️  **不含外部内存支持**

### 技术原因

wgpu 的设计哲学是提供**完全抽象**的跨平台 API,避免暴露特定后端细节。外部内存导入高度依赖:
- Vulkan: `VkExternalMemoryImageCreateInfo` + `vkImportMemoryFdKHR`
- Metal: `IOSurface` + `MTLTexture(iosurface:)`
- DX12: `ID3D12Device::OpenSharedHandle()`

这些 API 无法抽象为统一接口,而 wgpu 不愿破坏其跨平台保证。

### 后续路径

**选项 1: 等待官方支持** (不推荐)
- wgpu 团队可能在 v29+ 添加 experimental hal access
- 时间线未知,可能需要 6-12 个月
- **风险**: 等待期间用户体验无改善

**选项 2: 切换到 ash (raw Vulkan)** (代价太高)
- 需重写整个渲染管道 (`nv12_render.rs` 333 行 + egui 集成)
- 放弃 Metal/DX12 跨平台支持
- 估计工作量: **2-3 周**
- **收益**: 仅 12-20ms (不值得)

**选项 3: 执行 Phase 3 替代优化** (✅ **推荐**)
- 音频优化 (CPAL 独占 + 缓冲降低): 3-10ms
- 输入优化 (evdev 原始输入): 5-10ms
- **总收益**: 8-18ms (Phase 1 已获得 13-25ms)
- **累计**: 21-43ms 总延迟降低 (已满足用户需求)
- **实施成本**: 1 周 (远低于 ash 重写)

---

## 最终决定

**❌ Phase 2 零拷贝路径正式终止**

**理由**:
1. wgpu v28 未提供必需 API
2. 等待官方支持时间线未知且风险高
3. ash 重写成本过高 (2-3 周 vs 20ms 收益)
4. Phase 1 + 3 组合已足够 (21-43ms 总降低)

**下一步行动**:
- ✅ 提交 Phase 2 调研结果 (commit)
- ⏭️  启动 Phase 3 实施 (音频 + 输入优化)
- ⏭️  更新 `docs/LATENCY_OPTIMIZATION.md` 路线图
- ⏭️  在 wgpu GitHub 提 Feature Request (低优先级,长期跟踪)

---

**维护者**: SAide Development Team  
**最后更新**: 2026-01-15  
**最终状态**: ❌ **已终止** - wgpu v28 验证失败,转向 Phase 3 替代方案
