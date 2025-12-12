//! Test VAAPI hardware decoder

use {
    anyhow::Result,
    saide::{
        ScrcpyConnection,
        ServerParams,
        decoder::{VaapiDecoder, VideoDecoder},
        utils::get_device_serial,
    },
    tracing::{debug, info, warn},
};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    info!("Testing VAAPI hardware decoder...");

    let serial = get_device_serial()?;
    info!("Device: {}", serial);

    let server_jar = "3rd-party/scrcpy-server-v3.3.3";
    if !std::path::Path::new(server_jar).exists() {
        anyhow::bail!("Server JAR not found: {}", server_jar);
    }

    let params = ServerParams {
        video: true,
        video_codec: "h264".to_string(),
        video_bit_rate: 8_000_000,
        max_size: 1920,
        max_fps: 60,
        audio: false,
        control: true,
        send_device_meta: true,
        send_codec_meta: true,
        ..Default::default()
    };

    info!("Connecting to device...");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let mut conn =
        rt.block_on(async { ScrcpyConnection::connect(&serial, server_jar, params).await })?;

    info!("Connected! Device: {:?}", conn.device_name);

    // Get resolution
    let (width, height) = conn.video_resolution.unwrap_or((1920, 1080));
    info!("Video resolution: {}x{}", width, height);

    // Initialize VAAPI decoder
    info!("Initializing VAAPI decoder...");
    let mut decoder = VaapiDecoder::new(width, height)?;
    info!("VAAPI decoder initialized successfully!");

    // Decode test frames
    let target_frames = 10;
    let mut decoded_count = 0;
    let start_time = std::time::Instant::now();

    info!("Decoding {} frames...", target_frames);

    for _ in 0..100 {
        if decoded_count >= target_frames {
            break;
        }

        let packet = match conn.read_video_packet() {
            Ok(p) => p,
            Err(e) => {
                warn!("Failed to read packet: {}", e);
                break;
            }
        };

        if packet.data.is_empty() {
            continue;
        }

        match decoder.decode(&packet.data, packet.pts_us as i64) {
            Ok(Some(frame)) => {
                decoded_count += 1;
                debug!(
                    "[{}/{}] Decoded frame: {}x{} {:?} {} bytes",
                    decoded_count,
                    target_frames,
                    frame.width,
                    frame.height,
                    frame.format,
                    frame.data.len()
                );
            }
            Ok(None) => {
                // No frame yet
            }
            Err(e) => {
                warn!("Decode error: {}", e);
            }
        }
    }

    let elapsed = start_time.elapsed();
    let fps = decoded_count as f64 / elapsed.as_secs_f64();

    info!("\n=== VAAPI Decoder Test Results ===");
    info!("Frames decoded: {}", decoded_count);
    info!("Time elapsed: {:.2}s", elapsed.as_secs_f64());
    info!("Average FPS: {:.2}", fps);
    info!("Average frame time: {:.2}ms", 1000.0 / fps);

    conn.shutdown()?;
    info!("Test completed successfully!");

    Ok(())
}
