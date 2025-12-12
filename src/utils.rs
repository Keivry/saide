use anyhow::{Context, Result};

pub fn get_device_serial() -> Result<String> {
    if let Some(serial) = std::env::args().nth(1) {
        return Ok(serial);
    }

    let output = std::process::Command::new("adb")
        .args(["devices"])
        .output()
        .context("Failed to run 'adb devices'")?;

    let output_str = String::from_utf8_lossy(&output.stdout);

    for line in output_str.lines().skip(1) {
        if let Some(serial) = line.split_whitespace().next()
            && !serial.is_empty()
        {
            return Ok(serial.to_string());
        }
    }

    anyhow::bail!("No Android device found. Usage: render_nvdec <device_serial>")
}
