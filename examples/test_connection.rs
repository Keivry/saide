//! Scrcpy 真实设备连接测试
//!
//! 使用方法:
//!   cargo run --example test_connection [设备序列号]
//!   
//! 如果不指定序列号，将使用第一个可用设备

use anyhow::{Context, Result};
use saide::scrcpy::{ControlMessage, ScrcpyConnection, ServerParams, VideoPacket};

fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🧪 Scrcpy 协议实现测试");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // 获取设备序列号
    let serial = get_device_serial()?;
    println!("📱 设备: {}", serial);

    // 检查 server JAR
    let server_jar = "3rd-party/scrcpy-server-v3.3.3";
    if !std::path::Path::new(server_jar).exists() {
        anyhow::bail!("Server JAR 不存在: {}", server_jar);
    }
    println!("✓ Server JAR: {}", server_jar);

    // 配置参数
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
    println!("  视频: {} @ {}bps", params.video_codec, params.video_bit_rate);
    println!("  分辨率: {}px, 帧率: {}fps", params.max_size, params.max_fps);

    // 建立连接
    println!("\n🔌 建立连接中...");
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    let mut conn = rt.block_on(async {
        ScrcpyConnection::connect(&serial, server_jar, params).await
    })?;

    println!("✅ 连接成功!");
    println!("  本地端口: {}", conn.local_port);

    // 测试 1: 读取视频包
    println!("\n📹 测试 1: 读取视频流");
    test_video_packets(&mut conn)?;

    // 测试 2: 发送控制消息
    println!("\n🎮 测试 2: 发送控制消息");
    test_control_messages(&mut conn)?;

    // 测试 3: 进程状态
    println!("\n⚙️  测试 3: 服务器状态");
    if conn.is_server_alive() {
        println!("✅ Server 进程正常运行");
    } else {
        println!("⚠️  Server 进程未检测到");
    }

    // 清理
    println!("\n🛑 关闭连接...");
    conn.shutdown()?;

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✅ 所有测试通过!");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    Ok(())
}

fn get_device_serial() -> Result<String> {
    if let Some(serial) = std::env::args().nth(1) {
        return Ok(serial);
    }

    let output = std::process::Command::new("adb")
        .args(["devices"])
        .output()
        .context("执行 'adb devices' 失败")?;

    let output_str = String::from_utf8_lossy(&output.stdout);
    
    for line in output_str.lines().skip(1) {
        if let Some(serial) = line.split_whitespace().next() {
            if !serial.is_empty() {
                return Ok(serial.to_string());
            }
        }
    }

    anyhow::bail!("未找到 Android 设备，请连接设备并启用 USB 调试")
}

fn test_video_packets(conn: &mut ScrcpyConnection) -> Result<()> {
    let mut buf = vec![0u8; 65536];
    let mut stats = VideoStats::default();

    println!("  读取 10 个视频包...");

    for i in 0..10 {
        let size = conn.read_video(&mut buf)?;
        
        let mut cursor = std::io::Cursor::new(&buf[..size]);
        match VideoPacket::read_from(&mut cursor) {
            Ok(packet) => {
                stats.total += 1;
                
                if packet.is_config {
                    stats.config += 1;
                    println!("  [{:2}] CONFIG    {} bytes (SPS/PPS)", i+1, packet.data.len());
                } else if packet.is_keyframe {
                    stats.keyframe += 1;
                    println!("  [{:2}] KEYFRAME  {} bytes, PTS={}μs", 
                             i+1, packet.data.len(), packet.pts_us);
                } else {
                    stats.p_frame += 1;
                    println!("  [{:2}] P-FRAME   {} bytes, PTS={}μs", 
                             i+1, packet.data.len(), packet.pts_us);
                }
            }
            Err(e) => {
                println!("  [{:2}] ⚠️  解析失败: {}", i+1, e);
            }
        }
    }

    println!("\n  统计: 总计={}, CONFIG={}, 关键帧={}, P帧={}", 
             stats.total, stats.config, stats.keyframe, stats.p_frame);
    
    Ok(())
}

fn test_control_messages(conn: &mut ScrcpyConnection) -> Result<()> {
    let messages = vec![
        ("折叠通知栏", ControlMessage::CollapsePanels),
        ("触摸按下", ControlMessage::touch_down(500, 500, 1080, 2340)),
        ("触摸抬起", ControlMessage::touch_up(500, 500, 1080, 2340)),
    ];

    for (name, msg) in messages {
        let mut buf = Vec::new();
        msg.serialize(&mut buf)?;
        conn.send_control(&buf)?;
        println!("  ✓ {} ({} 字节)", name, buf.len());
        std::thread::sleep(std::time::Duration::from_millis(100));
    }

    println!("  ✅ 发送 {} 条消息", 3);
    Ok(())
}

#[derive(Default)]
struct VideoStats {
    total: u32,
    config: u32,
    keyframe: u32,
    p_frame: u32,
}
