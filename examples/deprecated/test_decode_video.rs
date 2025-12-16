//! 测试从真实设备解码视频流

use {
    anyhow::Result,
    saide::{
        ScrcpyConnection,
        ServerParams,
        decoder::{H264Decoder, VideoDecoder},
        utils::get_device_serial,
    },
    std::{fs::File, io::Write},
    tracing::{debug, info, warn},
};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🎬 Scrcpy 视频解码测试");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let serial = get_device_serial()?;
    println!("📱 设备: {}", serial);

    let server_jar = "3rd-party/scrcpy-server-v3.3.3";
    println!("✓ Server JAR: {}", server_jar);

    // 配置（send_codec_meta=true 以接收 SPS/PPS）
    // 使用 for_device() 自动加载 probe_codec 缓存的优化选项
    let mut params = ServerParams::for_device(&serial)?;
    params.video = true;
    params.video_codec = "h264".to_string();
    params.video_bit_rate = 8_000_000;
    params.max_size = 1600;
    params.max_fps = 60;
    params.audio = false;
    params.control = true;
    params.send_device_meta = true;
    params.send_codec_meta = true;

    println!("\n📋 配置:");
    println!("  编解码器: h264");
    println!("  最大分辨率: {}px", params.max_size);
    println!("  帧率: {}fps", params.max_fps);
    if let Some(ref opts) = params.video_codec_options {
        println!("  优化选项: {}", opts);
    } else {
        println!("  优化选项: None (使用默认配置)");
    }

    // 连接
    println!("\n🔌 建立连接...");
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let mut conn =
        rt.block_on(async { ScrcpyConnection::connect(&serial, server_jar, params).await })?;

    println!("✅ 连接成功");
    println!(
        "  设备名称: {}",
        conn.device_name.as_deref().unwrap_or("N/A")
    );

    // 读取第一个包
    println!("\n📹 读取视频流...");
    let first_packet = conn.read_video_packet()?;

    // 从第一个 config 包或实际帧获取分辨率
    // 简单起见，使用固定分辨率（实际应从 device meta 获取）
    let (width, height) = (1920, 1080);
    println!("  分辨率: {}x{}", width, height);

    // 初始化解码器
    println!("\n🎞️  初始化 H.264 解码器...");
    let mut decoder = H264Decoder::new(width, height)?;
    info!("解码器就绪");

    // 解码帧
    println!("\n🖼️  解码视频帧...");
    let target_frames = 5;
    let mut decoded_count = 0;
    let mut total_packets = 1; // 已读取第一个包
    let mut config_count = if first_packet.is_config { 1 } else { 0 };

    // 处理第一个包（包括 CONFIG）
    if !first_packet.data.is_empty() {
        if first_packet.is_config {
            info!("发送 SPS/PPS 到解码器");
        }
        match decoder.decode(&first_packet.data, first_packet.pts_us as i64) {
            Ok(Some(frame)) => {
                decoded_count += 1;
                println!(
                    "  [{}] ✅ {}x{} {:?}, {} bytes, PTS={}μs",
                    decoded_count,
                    frame.width,
                    frame.height,
                    frame.format,
                    frame.data.len(),
                    frame.pts
                );

                // 保存第一帧
                if decoded_count == 1 {
                    save_frame_as_ppm(&frame, "frame_001.ppm")?;
                    println!("    💾 已保存到 frame_001.ppm");
                }
            }
            Ok(None) => {}
            Err(e) => warn!("解码失败: {}", e),
        }
    }

    // 继续读取更多包
    for _ in 0..100 {
        if decoded_count >= target_frames {
            break;
        }

        let packet = match conn.read_video_packet() {
            Ok(p) => p,
            Err(e) => {
                warn!("读取包失败: {}", e);
                break;
            }
        };

        total_packets += 1;

        // CONFIG 包也要发送给解码器！
        if packet.is_config {
            config_count += 1;
            debug!("发送 SPS/PPS 到解码器");
            // 不要 continue，继续解码
        }

        if packet.data.is_empty() {
            continue;
        }

        // 解码（包括 CONFIG 包）
        match decoder.decode(&packet.data, packet.pts_us as i64) {
            Ok(Some(frame)) => {
                decoded_count += 1;
                println!(
                    "  [{}] ✅ {}x{} {:?}, {} bytes, PTS={}μs",
                    decoded_count,
                    frame.width,
                    frame.height,
                    frame.format,
                    frame.data.len(),
                    frame.pts
                );

                // 保存第一帧
                if decoded_count == 1 {
                    match save_frame_as_ppm(&frame, "/tmp/frame_001.ppm") {
                        Ok(_) => println!("    💾 已保存到 /tmp/frame_001.ppm"),
                        Err(e) => warn!("保存失败: {}", e),
                    }
                }
            }
            Ok(None) => {}
            Err(e) => warn!("解码失败: {}", e),
        }
    }

    // Flush
    println!("\n🔄 Flush 解码器...");
    match decoder.flush() {
        Ok(frames) => {
            for frame in frames {
                decoded_count += 1;
                println!(
                    "  [{}] ✅ (Flushed) {}x{} {:?}, PTS={}μs",
                    decoded_count, frame.width, frame.height, frame.format, frame.pts
                );
            }
        }
        Err(e) => warn!("Flush 失败: {}", e),
    }

    println!("\n📊 统计:");
    println!("  总包数: {}", total_packets);
    println!("  CONFIG 包: {}", config_count);
    println!("  解码帧数: {}", decoded_count);

    // 关闭
    println!("\n🛑 关闭连接...");
    conn.shutdown()?;

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✅ 测试完成!");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    Ok(())
}

fn save_frame_as_ppm(frame: &saide::decoder::DecodedFrame, filename: &str) -> Result<()> {
    let mut file = File::create(filename)?;
    writeln!(file, "P6")?;
    writeln!(file, "{} {}", frame.width, frame.height)?;
    writeln!(file, "255")?;

    // RGBA -> RGB
    for chunk in frame.data.chunks(4) {
        if chunk.len() >= 3 {
            file.write_all(&chunk[0..3])?;
        }
    }

    Ok(())
}
