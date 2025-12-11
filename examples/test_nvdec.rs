//! Test NVIDIA NVDEC hardware decoder

use {
    anyhow::{Context, Result},
    saide::{
        ScrcpyConnection,
        ServerParams,
        decoder::{NvdecDecoder, VideoDecoder},
    },
    std::process::Command,
    tracing::{info, warn},
};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .init();

    info!("Testing NVIDIA NVDEC hardware decoder...");

    let serial = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("Usage: test_nvdec <device_serial>"))?;

    info!("Device: {}", serial);

    let server_jar = "3rd-party/scrcpy-server-v3.3.3";
    let params = ServerParams::default();

    info!("Connecting...");
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    let mut conn =
        rt.block_on(async { ScrcpyConnection::connect(&serial, server_jar, params).await })?;

    let (width, height) = conn.video_resolution.unwrap_or((1920, 1080));
    info!("Resolution: {}x{}", width, height);

    info!("Initializing NVDEC...");
    let mut decoder = NvdecDecoder::new(width, height)?;

    let mut count = 0;
    let start = std::time::Instant::now();

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

    let fps = count as f64 / start.elapsed().as_secs_f64();
    info!("=== Results: {} frames, {:.2} FPS ===", count, fps);
    conn.shutdown()?;
    Ok(())
}
