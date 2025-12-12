//! Audio quality diagnostic tool

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

    let serial = get_device_serial()?;
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🔍 Audio Quality Diagnostic");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Device: {}", serial);

    let server_jar = "3rd-party/scrcpy-server-v3.3.3";
    let params = ServerParams {
        video: false,
        audio: true,
        audio_codec: "opus".to_string(),
        control: false,
        send_device_meta: false,
        send_codec_meta: false,
        send_frame_meta: true,
        log_level: "debug".to_string(),
        ..Default::default()
    };

    println!("\n🔌 Connecting...");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let mut conn =
        rt.block_on(async { ScrcpyConnection::connect(&serial, server_jar, params).await })?;

    if conn.audio_stream.is_none() {
        println!("⚠️  Audio not available");
        return Ok(());
    }

    println!("\n🎧 Initializing audio (48kHz stereo)...");
    let mut decoder = OpusDecoder::new(48000, 2)?;
    let player = AudioPlayer::new(48000, 2)?;

    println!("\n📊 Collecting samples (5 seconds)...\n");
    let start_time = std::time::Instant::now();
    let mut stats = Stats::default();

    while start_time.elapsed() < Duration::from_secs(5) {
        match conn.read_audio_packet() {
            Ok(audio_packet) => {
                stats.packets_received += 1;
                stats.total_bytes += audio_packet.payload.len();

                if stats.packets_received <= 3 {
                    println!(
                        "Packet #{}: {} bytes, PTS={}ms",
                        stats.packets_received,
                        audio_packet.payload.len(),
                        audio_packet.pts / 1000
                    );
                }

                match decoder.decode(&audio_packet.payload, audio_packet.pts) {
                    Ok(Some(decoded_audio)) => {
                        stats.frames_decoded += 1;
                        stats.total_samples += decoded_audio.samples.len();

                        if stats.frames_decoded <= 3 {
                            let duration_ms =
                                (decoded_audio.samples.len() / 2) as f32 / 48000.0 * 1000.0;
                            println!(
                                "  → Decoded: {} samples ({:.1}ms), rate={}Hz, ch={}",
                                decoded_audio.samples.len(),
                                duration_ms,
                                decoded_audio.sample_rate,
                                decoded_audio.channels
                            );

                            // Check for clipping or unusual values
                            let (min, max) = decoded_audio
                                .samples
                                .iter()
                                .fold((f32::MAX, f32::MIN), |(min, max), &s| {
                                    (min.min(s), max.max(s))
                                });
                            println!("     Range: [{:.6}, {:.6}]", min, max);

                            if max > 1.0 || min < -1.0 {
                                println!("     ⚠️  CLIPPING DETECTED!");
                                stats.clipping_frames += 1;
                            }
                        }

                        if let Err(e) = player.play(&decoded_audio) {
                            stats.playback_errors += 1;
                            if stats.playback_errors <= 3 {
                                eprintln!("     ⚠️  Playback error: {}", e);
                            }
                        }
                    }
                    Ok(None) => {
                        stats.skipped_packets += 1;
                    }
                    Err(e) => {
                        stats.decode_errors += 1;
                        if stats.decode_errors <= 3 {
                            eprintln!("  ⚠️  Decode error: {}", e);
                        }
                    }
                }

                // Sample buffer status every second
                let elapsed = start_time.elapsed().as_secs_f32();
                if ((elapsed * 10.0) as u32).is_multiple_of(10)
                    && stats.last_report_sec != elapsed as u32
                {
                    stats.last_report_sec = elapsed as u32;
                    println!(
                        "[{:.0}s] Buffer: {:.1}%, Packets: {}, Decoded: {}",
                        elapsed,
                        player.buffer_level() * 100.0,
                        stats.packets_received,
                        stats.frames_decoded
                    );
                }
            }
            Err(e) => {
                eprintln!("⚠️  Read error: {}", e);
                break;
            }
        }
    }

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📈 Diagnostic Results:");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("Packets received:  {}", stats.packets_received);
    println!("Packets skipped:   {}", stats.skipped_packets);
    println!("Frames decoded:    {}", stats.frames_decoded);
    println!("Decode errors:     {}", stats.decode_errors);
    println!("Playback errors:   {}", stats.playback_errors);
    println!("Clipping frames:   {}", stats.clipping_frames);
    println!();
    println!("Total bytes:       {} KB", stats.total_bytes / 1024);
    println!("Total samples:     {}", stats.total_samples);
    println!(
        "Avg packet size:   {} bytes",
        stats.total_bytes / stats.packets_received
    );
    println!(
        "Avg samples/frame: {}",
        stats.total_samples / stats.frames_decoded
    );
    println!(
        "Success rate:      {:.1}%",
        (stats.frames_decoded as f32 / stats.packets_received as f32) * 100.0
    );

    if stats.clipping_frames > 0 {
        println!("\n⚠️  WARNING: Audio clipping detected!");
        println!("   This may cause distortion. Check volume levels.");
    }

    if stats.playback_errors > 0 {
        println!("\n⚠️  WARNING: Playback errors detected!");
        println!("   Buffer may be overflowing or underflowing.");
    }

    player.stop();
    conn.shutdown()?;

    Ok(())
}

#[derive(Default)]
struct Stats {
    packets_received: usize,
    skipped_packets: usize,
    frames_decoded: usize,
    decode_errors: usize,
    playback_errors: usize,
    clipping_frames: usize,
    total_bytes: usize,
    total_samples: usize,
    last_report_sec: u32,
}
