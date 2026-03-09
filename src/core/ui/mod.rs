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
    crate::shortcut::{ShortcutManager, ShortcutMap, shortcut_map},
    lazy_static::lazy_static,
    parking_lot::RwLock,
    std::sync::Arc,
};
pub use {app::SAideApp, theme::ThemeMode};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppCommand {
    ShowHelp,
    ShowProfileSelection,
    PrevProfile,
    NextProfile,
    ShowRenameDialog,
    ShowCreateDialog,
    ShowDeleteDialog,
    ShowSaveAsDialog,
    CloseEditor,
    RenameProfile,
    CreateProfile,
    SaveProfileAs,
    DeleteProfile,
    SwitchProfile,
}

lazy_static! {
    pub static ref GLOBAL_SHORTCUTS: Arc<RwLock<ShortcutMap<AppCommand>>> =
        Arc::new(RwLock::new(shortcut_map! {
            "F1" => AppCommand::ShowHelp,
            "F6" => AppCommand::ShowProfileSelection,
            "F7" => AppCommand::PrevProfile,
            "F8" => AppCommand::NextProfile,
        }));
    pub static ref SHORTCUT_MANAGER: ShortcutManager<AppCommand> =
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
