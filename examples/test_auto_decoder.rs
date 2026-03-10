//! Test automatic GPU detection and decoder selection

mod utils;
use {
    anyhow::Result,
    saide::{
        decoder::{AutoDecoder, VideoDecoder},
        gpu::detect_gpu,
        scrcpy::{connection::ScrcpyConnection, server::ServerParams},
    },
    tracing::info,
    utils::{get_device_serial, get_scrcpy_server_path},
};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    info!("=== GPU Auto Detection Test ===");

    let gpu = detect_gpu();
    info!("Detected GPU: {:?}", gpu);

    let serial = get_device_serial()?;

    let server_jar = get_scrcpy_server_path()?;
    let params = ServerParams::default();

    info!("Connecting to device...");
    let _rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let mut conn = ScrcpyConnection::connect(&serial, &server_jar, "127.0.0.1", params)?;

    let (width, height) = conn.video_resolution.unwrap_or((1920, 1080));
    info!("Video resolution: {}x{}", width, height);

    info!("Initializing decoder with auto-detection...");
    let mut decoder = AutoDecoder::new(width, height, true)?;
    info!("Using {} decoder", decoder.decoder_type());

    let mut count = 0;
    let start = std::time::Instant::now();

    info!("Decoding 10 frames...");
    for _ in 0..100 {
        if count >= 10 {
            break;
        }

        let pkt = match conn.read_video_packet() {
            Ok(p) if !p.data.is_empty() => p,
            _ => continue,
        };

        if let Ok(Some(f)) = decoder.decode(&pkt.data, pkt.pts_us as i64) {
            count += 1;
            info!("[{}/10] {}x{} {:?}", count, f.width, f.height, f.format);
        }
    }

    let elapsed = start.elapsed();
    let fps = count as f64 / elapsed.as_secs_f64();

    info!("\n=== Results ===");
    info!("Decoder type: {}", decoder.decoder_type());
    info!("Frames: {}", count);
    info!("Time: {:.2}s", elapsed.as_secs_f64());
    info!("FPS: {:.2}", fps);

    conn.shutdown()?;
    info!("Test completed!");
    Ok(())
}
