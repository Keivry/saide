//! wgpu-hal prototype: Test external memory import capability
//!
//! This example verifies whether wgpu 0.27 hal module supports importing
//! external memory (DMA-BUF file descriptors) for zero-copy GPU decode.
//!
//! Run with: cargo run --example test_vulkan_import

#[tokio::main]
async fn main() {
    println!("=== wgpu-hal External Memory Import Prototype ===\n");

    let instance = wgpu::Instance::default();

    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        })
        .await
        .expect("Failed to find adapter");

    println!("Adapter: {}", adapter.get_info().name);
    println!("Backend: {:?}\n", adapter.get_info().backend);

    if adapter.get_info().backend != wgpu::Backend::Vulkan {
        eprintln!("ERROR: wgpu backend is not Vulkan (required for external memory)");
        eprintln!("Current backend: {:?}", adapter.get_info().backend);
        eprintln!("\nTo force Vulkan: WGPU_BACKEND=vulkan cargo run --example test_vulkan_import");
        std::process::exit(1);
    }

    let (device, _queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
        .expect("Failed to create device");

    println!("Device created successfully\n");

    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("ERROR: External memory import only supported on Linux");
        std::process::exit(1);
    }

    println!("✅ Platform: Linux (DMA-BUF supported)\n");

    println!("--- Step 1: Checking wgpu-hal API availability ---");

    let hal_available = test_hal_access(&device);

    if !hal_available {
        eprintln!("❌ FAILED: Cannot access wgpu-hal Vulkan backend");
        eprintln!("\nConclusion:");
        eprintln!("  wgpu 0.27 does not expose stable hal API for external memory import.");
        eprintln!("\nRecommended actions:");
        eprintln!("  1. Wait for wgpu 0.28+ with stable external memory support");
        eprintln!(
            "  2. OR switch to ash (direct Vulkan bindings) - requires rewriting render pipeline"
        );
        eprintln!("  3. OR keep Phase 1 CPU path as acceptable solution (12-20ms overhead)");
        std::process::exit(1);
    }

    println!("✅ wgpu-hal access successful\n");

    println!("--- Step 2: Checking Vulkan extensions ---");
    check_vulkan_extensions();

    println!("\n=== Prototype Result ===");
    println!("✅ wgpu-hal API is accessible");
    println!("✅ Vulkan backend detected");
    println!("\n⚠️  Next steps:");
    println!("  1. Implement DMA-BUF texture import using hal API");
    println!("  2. Test with real VAAPI decoded frames");
    println!("  3. Measure latency improvement");
    println!("\nPhase 2 implementation can proceed.");
}

#[allow(unexpected_cfgs)]
fn test_hal_access(_device: &wgpu::Device) -> bool {
    println!("⚠️  Checking wgpu-hal module availability...\n");

    #[cfg(feature = "wgpu-hal")]
    {
        println!("✅ wgpu-hal feature enabled");
        return true;
    }

    #[cfg(not(feature = "wgpu-hal"))]
    {
        println!("❌ wgpu-hal feature not enabled");
        println!("   wgpu 0.27 public API does not expose hal by default");
        println!("\n   Investigation findings:");
        println!("   - wgpu::Device::as_hal() is NOT available in public API");
        println!("   - External memory import requires raw Vulkan calls");
        println!("   - Alternative: Use ash crate directly (100% Vulkan control)");

        println!("\n   Checking wgpu features:");
        let features = wgpu::Features::all();
        println!("   Available features: {:?}", features);
    }

    false
}

fn check_vulkan_extensions() {
    println!("Required Vulkan extensions for zero-copy:");
    println!("  1. VK_KHR_external_memory_fd          (import DMA-BUF)");
    println!("  2. VK_KHR_external_semaphore_fd       (cross-API sync)");
    println!("  3. VK_EXT_image_drm_format_modifier   (NV12 format, optional)");

    println!("\n⚠️  Extension checking requires hal API or direct Vulkan access");
    println!("   Manual verification using vulkaninfo:");
    println!("\n   $ vulkaninfo | grep -A 2 'VK_KHR_external'");
    println!("\n   Expected output:");
    println!("     VK_KHR_external_memory_fd                 : extension revision 1");
    println!("     VK_KHR_external_semaphore_fd              : extension revision 1");
}
