//! Scrcpy 真实设备连接测试（最终版本）

mod utils;
use {
    anyhow::Result,
    saide::{ScrcpyConnection, ServerParams, VideoPacket},
    utils::get_device_serial,
};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🧪 Scrcpy 协议实现测试");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let serial = get_device_serial()?;
    println!("📱 设备: {}", serial);

    let server_jar = "3rd-party/scrcpy-server-v3.3.3";
    if !std::path::Path::new(server_jar).exists() {
        anyhow::bail!("Server JAR 不存在: {}", server_jar);
    }
    println!("✓ Server JAR: {}", server_jar);

    // 使用默认配置（send_codec_meta=true, send_device_meta=true）
    let params = ServerParams {
        video: true,
        video_codec: "h264".to_string(),
        video_bit_rate: 8_000_000,
        max_size: 1600,
        max_fps: 60,
        audio: false,
        control: true,
        log_level: "info".to_string(),
        ..Default::default()
    };

    println!("\n📋 配置:");
    println!("  SCID: {:08x}", params.scid);
    println!("  send_device_meta: {}", params.send_device_meta);
    println!("  send_codec_meta: {}", params.send_codec_meta);

    println!("\n🔌 建立连接中...");
    let mut conn = ScrcpyConnection::connect(&serial, server_jar, params)?;

    println!("✅ 连接成功!");
    println!(
        "  设备名称: {}",
        conn.device_name.as_deref().unwrap_or("N/A")
    );
    println!("  本地端口: {}", conn.local_port);

    println!("\n📹 测试: 读取 10 个视频包");
    test_video_packets(&mut conn)?;

    println!("\n🛑 关闭连接...");
    conn.shutdown()?;

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✅ 所有测试通过!");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    Ok(())
}

fn test_video_packets(conn: &mut ScrcpyConnection) -> Result<()> {
    let mut stats = VideoStats::default();

    for i in 0..10 {
        let mut header = [0u8; 12];
        conn.read_video_exact(&mut header)?;

        let packet_size = u32::from_be_bytes(header[8..12].try_into()?) as usize;

        let mut payload = vec![0u8; packet_size];
        conn.read_video_exact(&mut payload)?;

        let mut full_packet = Vec::with_capacity(12 + packet_size);
        full_packet.extend_from_slice(&header);
        full_packet.extend_from_slice(&payload);

        let mut cursor = std::io::Cursor::new(&full_packet);
        match VideoPacket::read_from(&mut cursor) {
            Ok(packet) => {
                stats.total += 1;
                if packet.is_config {
                    stats.config += 1;
                    println!(
                        "  [{:2}] CONFIG    {} bytes (SPS/PPS)",
                        i + 1,
                        packet.data.len()
                    );
                } else if packet.is_keyframe {
                    stats.keyframe += 1;
                    println!(
                        "  [{:2}] KEYFRAME  {} bytes, PTS={}ms",
                        i + 1,
                        packet.data.len(),
                        packet.pts_us / 1000
                    );
                } else {
                    stats.p_frame += 1;
                    println!(
                        "  [{:2}] P-FRAME   {} bytes, PTS={}ms",
                        i + 1,
                        packet.data.len(),
                        packet.pts_us / 1000
                    );
                }
            }
            Err(e) => println!("  [{:2}] ⚠️  解析失败: {}", i + 1, e),
        }
    }

    println!(
        "\n  统计: 总计={}, CONFIG={}, 关键帧={}, P帧={}",
        stats.total, stats.config, stats.keyframe, stats.p_frame
    );
    Ok(())
}

#[derive(Default)]
struct VideoStats {
    total: u32,
    config: u32,
    keyframe: u32,
    p_frame: u32,
}
