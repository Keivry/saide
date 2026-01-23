//! Test codec options auto-detection

use {
    anyhow::{Context, Result},
    saide::scrcpy::codec_probe,
    std::process::Command,
};

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("🔍 Video Codec Options Auto-Detection");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");

    // Get device serial
    let serial = get_device_serial()?;
    println!("\n📱 Device: {}", serial);

    let server_jar = "3rd-party/scrcpy-server-v3.3.3";
    if !std::path::Path::new(server_jar).exists() {
        anyhow::bail!("Server JAR not found: {}", server_jar);
    }

    // Probe device
    println!("\n🚀 Starting compatibility probe...\n");
    let optimal_config = codec_probe::probe_device(&serial, server_jar)?;

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

fn get_device_serial() -> Result<String> {
    // Check CLI argument first
    if let Some(serial) = std::env::args().nth(1) {
        return Ok(serial);
    }

    // Fallback to `adb devices`
    let output = Command::new("adb")
        .args(["devices", "-l"])
        .output()
        .context("Failed to run 'adb devices'")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines().skip(1) {
        if line.trim().is_empty() {
            continue;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[1] == "device" {
            return Ok(parts[0].to_string());
        }
    }

    anyhow::bail!("No device found. Usage: cargo run --example probe_codec [serial]")
}
