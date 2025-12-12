//! Test audio streaming from real device

use {
    anyhow::Result,
    saide::{
        ScrcpyConnection,
        ServerParams,
        decoder::{AudioDecoder, AudioPlayer, OpusDecoder},
        utils::get_device_serial,
    },
    std::time::Duration,
};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🎵 Scrcpy Audio Streaming Test");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let serial = get_device_serial()?;
    println!("📱 Device: {}", serial);

    let server_jar = "3rd-party/scrcpy-server-v3.3.3";
    if !std::path::Path::new(server_jar).exists() {
        anyhow::bail!("Server JAR not found: {}", server_jar);
    }

    // Enable audio streaming
    let params = ServerParams {
        video: false, // Disable video for audio-only test
        audio: true,
        audio_codec: "opus".to_string(),
        control: false,
        send_device_meta: false,
        send_codec_meta: false,
        send_frame_meta: true,
        log_level: "info".to_string(),
        ..Default::default()
    };

    println!("\n📋 Configuration:");
    println!("  SCID: {:08x}", params.scid);
    println!("  Audio codec: {}", params.audio_codec);
    println!("  Video: disabled");

    println!("\n🔌 Establishing connection...");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let mut conn =
        rt.block_on(async { ScrcpyConnection::connect(&serial, server_jar, params).await })?;

    println!("✅ Connection established!");

    // Check if audio is actually available
    if conn.audio_stream.is_none() {
        println!("\n⚠️  Audio not available:");
        println!("   - Device requires Android 11+ (API 30+)");
        println!("   - Or audio was explicitly disabled");
        println!("\n💡 Tip: Use a device with Android 11+ to test audio streaming");
        return Ok(());
    }

    // Initialize Opus decoder and audio player
    println!("\n🎧 Initializing audio...");
    let mut decoder = OpusDecoder::new(48000, 2)?;
    let player = AudioPlayer::new(48000, 2)?;

    println!("✅ Audio initialized: 48kHz stereo (libopus)");

    // Stream audio for 10 seconds
    println!("\n🎵 Streaming audio (10 seconds)...");
    let start_time = std::time::Instant::now();
    let mut packet_count = 0;
    let mut decode_count = 0;

    while start_time.elapsed() < Duration::from_secs(10) {
        match conn.read_audio_packet() {
            Ok(audio_packet) => {
                packet_count += 1;

                // Decode audio packet
                match decoder.decode(&audio_packet.payload, audio_packet.pts) {
                    Ok(Some(decoded_audio)) => {
                        decode_count += 1;

                        // Play audio
                        if let Err(e) = player.play(&decoded_audio) {
                            eprintln!("⚠️  Failed to play audio: {}", e);
                        }

                        if packet_count % 50 == 0 {
                            println!(
                                "  📊 Packets: {}, Decoded: {}, Buffer: {:.1}%",
                                packet_count,
                                decode_count,
                                player.buffer_level() * 100.0
                            );
                        }
                    }
                    Ok(None) => {
                        // Need more data or skipped (e.g., config packet)
                    }
                    Err(e) => {
                        // Only log errors for non-first packets (first might be config)
                        if packet_count > 1 {
                            eprintln!("⚠️  Failed to decode audio packet #{}: {}", packet_count, e);
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("⚠️  Failed to read audio packet: {}", e);
                break;
            }
        }
    }

    println!("\n📊 Statistics:");
    println!("  Total packets: {}", packet_count);
    println!("  Decoded frames: {}", decode_count);
    println!("  Duration: {:.1}s", start_time.elapsed().as_secs_f32());

    println!("\n🛑 Stopping playback...");
    player.stop();

    println!("\n🛑 Closing connection...");
    conn.shutdown()?;

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✅ Test completed!");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    Ok(())
}
