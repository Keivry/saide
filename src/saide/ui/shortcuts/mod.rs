#[macro_use]
mod manager;

pub(crate) use manager::{ShortcutMap, shortcut};
use {crate::saide::SAideApp, lazy_static::lazy_static};

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
    pub static ref MAPPING_EDIT_SHORTCUTS: ShortcutMap<SAideApp> = shortcuts! {
        sc!("F2") => action!(serial [
            action!(ui string SAideApp::show_rename_profile_dialog),
            action!(func string SAideApp::rename_profile),
        ]);
        sc!("F3") => action!(serial [
            action!(ui string SAideApp::show_create_profile_dialog),
            action!(func string SAideApp::create_profile),
        ]);
        sc!("F4") => action!(serial [
            action!(ui bool SAideApp::show_delete_profile_dialog),
            action!(func SAideApp::delete_profile),
        ]);
        sc!("F5") => action!(serial [
            action!(ui string SAideApp::show_save_profile_as_dialog),
            action!(func string SAideApp::save_profile_as),
        ]);
    };
}
