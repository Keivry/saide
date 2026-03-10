//! Test codec options auto-detection

mod utils;

use {
    anyhow::Result,
    saide::scrcpy::codec_probe,
    utils::{get_device_serial, get_scrcpy_server_path},
};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🔍 Video Codec Options Auto-Detection");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    let serial = get_device_serial()?;
    println!("\n📱 Device: {}", serial);

    let server_jar = get_scrcpy_server_path()?;

    // Probe device
    println!("\n🚀 Starting compatibility probe...\n");
    let optimal_config = codec_probe::probe_device(&serial, &server_jar, None)?;

    println!("\n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("✅ Probe Complete!");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    if let Some(config) = optimal_config {
        println!("\n📋 Recommended configuration:");
        println!("   video_codec_options: Some(\"{}\".to_string())", config);
    } else {
        println!("\n⚠️  No compatible options found.");
        println!("   Recommendation: Use video_codec_options: None");
    }

    // Show saved profile location
    use saide::constant::{config_dir, fallback_data_path};
    let config_path = config_dir()
        .and_then(|p: std::path::PathBuf| {
            p.parent().map(|parent| parent.join("device_profiles.toml"))
        })
        .or_else(|| Some(fallback_data_path().join("device_profiles.toml")));

    if let Some(path) = config_path {
        println!("\n💾 Profile saved to: {}", path.display());
        println!("   (Future connections will use cached config)");
    }

    Ok(())
}
