//! 协议实现验证测试（无需真实设备）
//!
//! 测试控制协议和视频协议的正确性

use saide::scrcpy::{ControlMessage, VideoPacket, protocol::control::AndroidKeyEventAction};

fn main() -> anyhow::Result<()> {
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🧪 Scrcpy 协议实现验证测试");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    test_control_protocol()?;
    test_video_protocol()?;

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✅ 所有协议测试通过!");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    Ok(())
}

fn test_control_protocol() -> anyhow::Result<()> {
    println!("📋 测试 1: 控制协议序列化\n");

    let test_cases = vec![
        (
            "Touch Down",
            ControlMessage::touch_down(100, 200, 1080, 2340),
            32,
        ),
        (
            "Touch Move",
            ControlMessage::touch_move(150, 250, 1080, 2340),
            32,
        ),
        (
            "Touch Up",
            ControlMessage::touch_up(200, 300, 1080, 2340),
            32,
        ),
        ("Key Down (BACK)", ControlMessage::key_down(4, 0), 14),
        ("Key Up (BACK)", ControlMessage::key_up(4, 0), 14),
        (
            "Text Injection",
            ControlMessage::InjectText {
                text: "Hello".to_string(),
            },
            10,
        ),
        (
            "Scroll",
            ControlMessage::scroll(500, 500, 1080, 2340, 0.0, 5.0),
            21,
        ),
        ("Collapse Panels", ControlMessage::CollapsePanels, 1),
        (
            "Back or Screen On",
            ControlMessage::BackOrScreenOn {
                action: AndroidKeyEventAction::Down,
            },
            2,
        ),
    ];

    for (name, msg, expected_size) in &test_cases {
        let mut buf = Vec::new();
        msg.serialize(&mut buf)?;

        if buf.len() == *expected_size {
            println!("  ✓ {}: {} 字节", name, buf.len());
        } else {
            println!(
                "  ✗ {}: 期望 {} 字节, 实际 {} 字节",
                name,
                expected_size,
                buf.len()
            );
            anyhow::bail!("Size mismatch for {}", name);
        }
    }

    let count = test_cases.len();
    println!("  ✅ 控制协议测试通过 ({} 种消息)\n", count);
    Ok(())
}

fn test_video_protocol() -> anyhow::Result<()> {
    println!("📹 测试 2: 视频协议解析\n");

    // 测试用例：模拟 scrcpy server 发送的包格式
    let test_cases = vec![
        (
            "Config Packet (SPS/PPS)",
            create_video_packet(0, true, false, &[0x67, 0x42, 0x00, 0x1f]),
        ),
        (
            "Keyframe (IDR)",
            create_video_packet(1_000_000, false, true, &[0x65, 0x88, 0x84, 0x00]),
        ),
        (
            "P-frame",
            create_video_packet(2_000_000, false, false, &[0x61, 0x9a]),
        ),
    ];

    for (name, packet_bytes) in &test_cases {
        let mut cursor = std::io::Cursor::new(&packet_bytes);
        let packet = VideoPacket::read_from(&mut cursor)?;

        println!("  ✓ {}", name);
        println!("      PTS: {}μs", packet.pts_us);
        println!(
            "      Config: {}, Keyframe: {}",
            packet.is_config, packet.is_keyframe
        );
        println!("      数据: {} 字节", packet.data.len());
    }

    let count = test_cases.len();
    println!("  ✅ 视频协议测试通过 ({} 个包)\n", count);
    Ok(())
}

fn create_video_packet(pts_us: u64, is_config: bool, is_keyframe: bool, payload: &[u8]) -> Vec<u8> {
    use byteorder::{BigEndian, WriteBytesExt};

    let mut buf = Vec::new();

    // pts_and_flags
    let mut pts_and_flags = pts_us;
    if is_config {
        pts_and_flags |= 1 << 63;
    }
    if is_keyframe {
        pts_and_flags |= 1 << 62;
    }
    buf.write_u64::<BigEndian>(pts_and_flags).unwrap();

    // packet_size
    buf.write_u32::<BigEndian>(payload.len() as u32).unwrap();

    // payload
    buf.extend_from_slice(payload);

    buf
}
