// SPDX-License-Identifier: MIT OR Apache-2.0

//! Test codec options auto-detection

mod utils;

use {
    anyhow::Result,
    saide::decoder_probe,
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
    let optimal_config = decoder_probe::probe_device(&serial, &server_jar, None)?;

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

    use saide::constant::config_dir;
    let config_dir = config_dir();

    println!(
        "\n💾 Encoder profile: {}",
        config_dir.join("encoder_profile.toml").display()
    );
    println!(
        "💾 Decoder profile: {}",
        config_dir.join("decoder_profile.toml").display()
    );
    println!("   (Future connections will reuse cached encoder and decoder results)");

    Ok(())
}
