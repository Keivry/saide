//! Thread-safe i18n manager with Fluent
//!
//! Features:
//! - Debug: Load FTL from filesystem with hot reload
//! - Release: Embed FTL into binary, no filesystem dependency
//! - Auto-scan i18n folder for available locales
//! - Support multiple FTL files per locale (auto-merged)
//! - Hot reload: watch FTL files and auto-reload bundles (debug only)
//!
//! Useage:
//! ```rust
//! // Get localized message by key
//! let msg = t!("message-key");
//!
//! // Get localized message with arguments
//! let msg_with_args = tf!("message-key", "arg1" => val1, "arg2" => val2);
//! ```

mod manager;
mod source;

#[cfg(debug_assertions)]
mod fs_source;

#[cfg(not(debug_assertions))]
mod embedded;

#[cfg(debug_assertions)]
pub use fs_source::FtlWatcherSource;
pub use {
    manager::{I18nManager, L10N},
    source::FtlSource,
};

/// Shorthand macro to get a localized message by key.
/// Usage: `t!("message-key")`
#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::i18n::L10N.read().get($key)
    };
}

/// Shorthand macro to get a localized message with arguments.
/// Usage: `tf!("message-key", "arg1" => val1, "arg2" => val2)`
#[macro_export]
macro_rules! tf {
    ($key:expr, $($arg_key:expr => $arg_val:expr),+ $(,)?) => {{
        let mut args = fluent_bundle::FluentArgs::new();
        $(
            args.set($arg_key, $arg_val);
        )+
        $crate::i18n::L10N.read().get_with_fluent_args($key, Some(&args))
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_locale_detection() {
        let manager = I18nManager::new();
        let locales = manager.available_locales();
        assert!(!locales.is_empty());
        assert!(locales.contains(&manager.current_locale()));
    }

    #[test]
    fn test_get_message() {
        let mut manager = I18nManager::new();
        manager.set_locale("en-US");
        let msg = manager.get("app-title");
        assert_eq!(msg, "SAide");
    }

    #[test]
    fn test_get_with_fluent_args() {
        use fluent_bundle::FluentArgs;
        let mut manager = I18nManager::new();
        manager.set_locale("en-US");
        let mut args = FluentArgs::new();
        args.set("backend", "Vulkan");
        let msg = manager.get_with_fluent_args("config-video-backend", Some(&args));
        assert!(msg.contains("Vulkan"));
    }

    #[test]
    fn test_is_chinese() {
        let mut manager = I18nManager::new();
        manager.set_locale("zh-CN");
        assert!(manager.is_chinese());
        manager.set_locale("en-US");
        assert!(!manager.is_chinese());
    }

    #[test]
    fn test_macro_t() {
        let msg = t!("app-title");
        assert_eq!(msg, "SAide");
    }

    #[test]
    fn test_macro_tf_with_number() {
        let msg = tf!("config-max-fps", "fps" => 60);
        assert!(msg.contains("60"));
    }

    #[test]
    fn test_locale_switch_performance() {
        let mut manager = I18nManager::new();
        for _ in 0..100 {
            manager.set_locale("zh-CN");
            manager.set_locale("en-US");
        }
    }

    #[test]
    fn test_concurrent_access() {
        use std::thread;
        let handles: Vec<_> = (0..10)
            .map(|i| {
                thread::spawn(move || {
                    if i % 2 == 0 {
                        t!("app-title")
                    } else {
                        tf!("config-max-fps", "fps" => i)
                    }
                })
            })
            .collect();
        for handle in handles {
            let _ = handle.join();
        }
    }

    #[test]
    fn test_available_locales() {
        let manager = I18nManager::new();
        let locales = manager.available_locales();
        assert!(locales.contains(&"en-US"));
        assert!(locales.contains(&"zh-CN"));
    }
}
