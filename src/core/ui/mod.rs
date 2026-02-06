mod editor;
mod indicator;
mod main;
mod player;
mod theme;
mod toolbar;

pub use main::SAideApp;
use {
    crate::{sc, shortcut::ShortcutMap, shortcuts},
    lazy_static::lazy_static,
};

lazy_static! {
    pub static ref DEFAULT_SHORTCUTS: ShortcutMap<SAideApp> = shortcuts! {
        sc!("F1") => action!(ui SAideApp::show_help_dialog);
        sc!("F6") => action!(serial [
            action!(ui usize SAideApp::show_profile_selection),
            action!(func usize SAideApp::switch_profile)
        ]);
        sc!("F7") => action!(func SAideApp::prev_profile);
        sc!("F8") => action!(func SAideApp::next_profile);
    };
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
