// SPDX-License-Identifier: MIT OR Apache-2.0

use {
    adbshell::AdbShell,
    crossbeam_channel::Receiver,
    eframe::{egui, egui_wgpu},
    egui_cjk_font::load_cjk_font,
    saide::{
        config::ConfigManager,
        core::ui::{AppShell, SAideApp, ThemeMode},
        error::{Result, SAideError},
        tf,
    },
    tracing::{info, warn},
    tracing_subscriber::{EnvFilter, fmt, prelude::*},
};

const WGPU_LOG_LEVEL: &str = "error";

fn main() -> Result<()> {
    let (config_manager, config_load_warning) = ConfigManager::new_or_default();

    let config = config_manager.config();
    let level = config.logging.level.as_str();

    // Build default filter from config, but allow RUST_LOG to override
    // Filter out verbose third-party logs unless explicitly debugging
    let default_filter = format!(
        "{},wgpu_hal={},wgpu_core={},naga={},eframe=info,winit=info,sctk=info,egui_wgpu=info",
        level,
        match level {
            "trace" => "debug",
            _ => WGPU_LOG_LEVEL,
        },
        WGPU_LOG_LEVEL,
        WGPU_LOG_LEVEL
    );

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer())
        .init();

    info!("SAide starting...");

    let mut startup_error: Option<String> = None;
    let mut startup_warnings: Vec<String> = Vec::new();

    if let Some(w) = config_load_warning {
        startup_warnings.push(tf!("notification-config-load-failed", "detail" => w.as_str()));
    }

    let mut serial = String::new();
    let (tx, rx) = crossbeam_channel::bounded(1);

    if let Err(adb_err) = AdbShell::verify_adb_available() {
        let e = SAideError::from(adb_err);
        warn!("ADB not available: {}", e);
        startup_error = Some(match &e {
            SAideError::AdbNotFound(detail) => {
                tf!("startup-error-adb-not-found", "detail" => detail.as_str())
            }
            SAideError::AdbCommandFailed(detail) => {
                tf!("startup-error-adb-command-failed", "detail" => detail.as_str())
            }
            _ => e.to_string(),
        });
    } else {
        info!("Video backend: {}", config.gpu.backend.to_string());
        info!(
            "Max video size: {}",
            config.scrcpy.video.max_size.to_string()
        );
        info!("Max FPS: {}", config.scrcpy.video.max_fps.to_string());
        info!("Logging level: {}", config.logging.level.as_str());

        let tx_clone = tx.clone();
        ctrlc::set_handler(move || {
            info!("Received Ctrl-C, shutting down...");
            let _ = tx_clone.send(());
        })
        .map_err(|e| SAideError::Other(format!("Failed to set Ctrl-C handler: {}", e)))?;

        match AdbShell::get_device_serial() {
            Ok(s) => {
                serial = s;
            }
            Err(adb_err) => {
                let e = SAideError::from(adb_err);
                warn!("Failed to get device serial: {}", e);
                if startup_error.is_none() {
                    startup_error = Some(match &e {
                        SAideError::AdbDeviceNotFound(detail) => {
                            tf!("startup-error-device-not-found", "detail" => detail.as_str())
                        }
                        _ => e.to_string(),
                    });
                }
            }
        };
    }

    start_ui(&serial, config_manager, rx, startup_error, startup_warnings)
}

fn start_ui(
    serial: &str,
    config_manager: ConfigManager,
    shutdown_rx: Receiver<()>,
    startup_error: Option<String>,
    startup_warnings: Vec<String>,
) -> Result<()> {
    let config = config_manager.config();
    let toolbar_width = if config.general.auto_hide_toolbar {
        0.0
    } else {
        SAideApp::toolbar_width()
    };
    let window_width = config.general.window_width;
    let window_height = config.general.window_height;

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("SAide")
            .with_inner_size([window_width as f32 + toolbar_width, window_height as f32]),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: egui_wgpu::WgpuConfiguration {
            present_mode: if config.gpu.vsync {
                wgpu::PresentMode::AutoVsync
            } else {
                wgpu::PresentMode::AutoNoVsync
            },

            wgpu_setup: egui_wgpu::WgpuSetup::from(egui_wgpu::WgpuSetupCreateNew {
                instance_descriptor: wgpu::InstanceDescriptor {
                    backends: (&config.gpu.backend).into(),
                    ..Default::default()
                },
                ..Default::default()
            }),

            desired_maximum_frame_latency: Some(1),

            ..Default::default()
        },
        ..Default::default()
    };

    let result = eframe::run_native(
        "SAide",
        options,
        Box::new(move |cc| {
            load_cjk_font(&cc.egui_ctx);
            ThemeMode::apply_to_context(&cc.egui_ctx);
            Ok(Box::new(AppShell::new(
                cc,
                serial,
                config_manager,
                shutdown_rx,
                startup_error,
                startup_warnings,
            )))
        }),
    );

    if let Err(ref e) = result {
        let _ = rfd::MessageDialog::new()
            .set_title("SAide — Fatal Error")
            .set_description(e.to_string())
            .show();
    }

    result.map_err(|e| SAideError::UiError(e.to_string()))
}
