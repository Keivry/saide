mod app;
mod dialog;
mod editor;
mod function;
mod indicator;
mod notifier;
mod player;
mod theme;
mod toolbar;

use {
    crate::{
        sc,
        shortcut::{ShortcutManager, ShortcutMap},
        shortcuts,
    },
    egui_action_macro::action,
    lazy_static::lazy_static,
    parking_lot::RwLock,
    std::sync::Arc,
};
pub use {app::SAideApp, theme::ThemeMode};

lazy_static! {
    pub static ref GLOBAL_SHORTCUTS: Arc<RwLock<ShortcutMap<SAideApp>>> =
        Arc::new(RwLock::new(shortcuts! {
            sc!("F1") => action!(null SAideApp::show_help_dialog);
            sc!("F6") => action!(serial [
                action!(null SAideApp::show_profile_selection_dialog),
                action!(null SAideApp::switch_profile)
            ]);
            sc!("F7") => action!(null SAideApp::prev_profile);
            sc!("F8") => action!(null SAideApp::next_profile);
        }));
    pub static ref SHORTCUT_MANAGER: ShortcutManager<SAideApp> =
        ShortcutManager::new(GLOBAL_SHORTCUTS.clone());
}

#[derive(Default)]
pub struct VideoStats {
    pub fps: f32,
    pub total_frames: u32,
    pub dropped_frames: u32,
    pub latency_ms: f32,
    pub latency_decode_ms: f32,
    pub latency_upload_ms: f32,
    pub latency_p95_ms: f32,
}
