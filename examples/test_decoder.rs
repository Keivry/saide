//! Test H.264 decoder with real scrcpy video stream

use anyhow::Result;
use saide::decoder::{H264Decoder, VideoDecoder};
use saide::scrcpy::{ScrcpyConnection, ServerParams, VideoPacket};
use std::time::Instant;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🎬 H.264 Decoder 测试");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Get device
    let serial = get_device_serial()?;
    println!("📱 设备: {}", serial);

    // Connect
    let params = ServerParams {
        max_size: 1280,  // Lower resolution for testing
        ..Default::default()
    };

    println!("\n🔌 连接设备...");
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let mut conn = rt.block_on(async {
        ScrcpyConnection::connect(&serial, "3rd-party/scrcpy-server-v3.3.3", params).await
    })?;

    println!("✅ 连接成功!");
    println!("   设备: {}", conn.device_name.as_deref().unwrap_or("Unknown"));

    // Create decoder (we'll get actual size from codec meta)
    println!("\n🎬 初始化解码器...");
    let mut decoder = H264Decoder::new(1280, 720)?;
    println!("✅ 解码器就绪");

    // Decode 10 frames
    println!("\n📹 解码测试 (10 帧):");
    let mut decoded_count = 0;
    let mut packet_count = 0;
    let start = Instant::now();

    while decoded_count < 10 && packet_count < 50 {
        // Read packet
        let mut header = [0u8; 12];
        conn.read_video_exact(&mut header)?;

        let packet_size = u32::from_be_bytes(header[8..12].try_into()?) as usize;
        let mut payload = vec![0u8; packet_size];
        conn.read_video_exact(&mut payload)?;

        let mut full_packet = Vec::with_capacity(12 + packet_size);
        full_packet.extend_from_slice(&header);
        full_packet.extend_from_slice(&payload);

        let packet = VideoPacket::read_from(&mut std::io::Cursor::new(&full_packet))?;
        packet_count += 1;

        // Decode
        match decoder.decode(&packet.data, packet.pts_us as i64) {
            Ok(Some(frame)) => {
                decoded_count += 1;
                let elapsed = start.elapsed().as_millis();
                println!(
                    "  [{:2}] ✅ {}x{} @ {}ms, PTS={}ms",
                    decoded_count,
                    frame.width,
                    frame.height,
                    elapsed,
                    frame.pts / 1000
                );
            }
            Ok(None) => {
                // Need more data (e.g., CONFIG packet)
                if packet.is_config {
                    println!("  [--] 📦 CONFIG packet ({} bytes)", packet.data.len());
                }
            }
            Err(e) => {
                println!("  [--] ❌ 解码失败: {}", e);
            }
        }
    }

    let total_time = start.elapsed();
    println!("\n📊 统计:");
    println!("  总包数: {}", packet_count);
    println!("  解码帧数: {}", decoded_count);
    println!("  总耗时: {:?}", total_time);
    if decoded_count > 0 {
        println!(
            "  平均解码时间: {:.2}ms",
            total_time.as_millis() as f64 / decoded_count as f64
        );
    }

    conn.shutdown()?;
    println!("\n✅ 测试完成!");

    Ok(())
}

fn get_device_serial() -> Result<String> {
    if let Some(serial) = std::env::args().nth(1) {
        return Ok(serial);
    }

    let output = std::process::Command::new("adb")
        .args(["devices"])
        .output()?;

    let output_str = String::from_utf8_lossy(&output.stdout);
    for line in output_str.lines().skip(1) {
        if let Some(serial) = line.split_whitespace().next() {
            if !serial.is_empty() {
                return Ok(serial.to_string());
            }
        }
    }

    anyhow::bail!("未找到 Android 设备")
}
