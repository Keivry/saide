use {
    anyhow::{Context, Result},
    saide::constant::resolve_scrcpy_server_path,
};

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

    anyhow::bail!("No Android device found. Usage: cargo run --example <example-name> [serial]")
}

pub fn get_scrcpy_server_path() -> Result<String> {
    let path = resolve_scrcpy_server_path();
    if path.is_file() {
        return Ok(path.to_string_lossy().to_string());
    }

    anyhow::bail!(
        "Scrcpy server JAR not found. Expected '{}' in the current directory, application data directory, or legacy 3rd-party/ path.",
        path.display()
    )
}
