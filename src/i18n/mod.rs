//! Thread-safe i18n manager with Fluent
//!
//! Improvements over basic version:
//! - Pre-load all bundles for better performance
//! - Support FluentArgs for type-safe parameters
//! - Better error handling with logging
//! - Thread-safe concurrent FluentBundle

use {
    fluent_bundle::{FluentArgs, FluentResource, FluentValue, bundle::FluentBundle},
    intl_memoizer::concurrent::IntlLangMemoizer,
    once_cell::sync::Lazy,
    parking_lot::RwLock,
    std::{collections::HashMap, sync::Arc},
    unic_langid::LanguageIdentifier,
};

// Include FTL files
const EN_US_FTL: &str = include_str!("../../i18n/en-US/main.ftl");
const ZH_CN_FTL: &str = include_str!("../../i18n/zh-CN/main.ftl");

// Supported locales
const AVAILABLE_LOCALES: &[&str] = &["en-US", "zh-CN"];

// Global thread-safe i18n manager
pub static L10N: Lazy<Arc<RwLock<I18nManager>>> =
    Lazy::new(|| Arc::new(RwLock::new(I18nManager::new())));

// Thread-safe FluentBundle type
type ThreadSafeFluentBundle = FluentBundle<FluentResource, IntlLangMemoizer>;

pub struct I18nManager {
    current_locale: String,
    bundles: HashMap<String, ThreadSafeFluentBundle>,
}

impl I18nManager {
    pub fn new() -> Self {
        // Pre-load all bundles for better performance
        let mut bundles = HashMap::new();
        for locale in AVAILABLE_LOCALES {
            bundles.insert(locale.to_string(), Self::load_bundle(locale));
        }

        let locale = Self::detect_locale();
        Self {
            current_locale: locale,
            bundles,
        }
    }

    /// Detect system locale with simple prefix matching
    fn detect_locale() -> String {
        let sys_locale = sys_locale::get_locale().unwrap_or_else(|| "en-US".to_string());

        // Simple matching: if system locale starts with "zh", use zh-CN
        if sys_locale.starts_with("zh") {
            "zh-CN".to_string()
        } else {
            "en-US".to_string()
        }
    }

    /// Load FluentBundle for a locale
    fn load_bundle(locale: &str) -> ThreadSafeFluentBundle {
        let ftl_content = match locale {
            "zh-CN" => ZH_CN_FTL,
            _ => EN_US_FTL,
        };

        let resource =
            FluentResource::try_new(ftl_content.to_string()).expect("Failed to parse FTL");
        let langid: LanguageIdentifier = locale.parse().unwrap();
        let mut bundle = FluentBundle::new_concurrent(vec![langid]);
        bundle
            .add_resource(resource)
            .expect("Failed to add FTL resource");
        bundle
    }

    /// Get current bundle
    fn current_bundle(&self) -> &ThreadSafeFluentBundle {
        self.bundles
            .get(&self.current_locale)
            .unwrap_or_else(|| self.bundles.get("en-US").unwrap())
    }

    /// Get current locale
    pub fn current_locale(&self) -> &str { &self.current_locale }

    /// Set locale dynamically (no re-parsing, just switch to pre-loaded bundle)
    pub fn set_locale(&mut self, locale: &str) {
        // Simple matching: normalize to supported locale
        let normalized = if locale.starts_with("zh") {
            "zh-CN"
        } else {
            "en-US"
        };

        // Only switch if bundle exists (it should, since we pre-loaded)
        if self.bundles.contains_key(normalized) {
            self.current_locale = normalized.to_string();
        }
    }

    /// Get localized string without arguments
    pub fn get(&self, key: &str) -> String { self.get_with_fluent_args(key, None) }

    /// Get localized string with FluentArgs (recommended for type safety)
    pub fn get_with_fluent_args(&self, key: &str, args: Option<&FluentArgs>) -> String {
        let bundle = self.current_bundle();
        let mut errors = vec![];

        bundle
            .get_message(key)
            .and_then(|msg| msg.value())
            .map(|pattern| {
                let result = bundle.format_pattern(pattern, args, &mut errors);

                if !errors.is_empty() {
                    eprintln!("Fluent formatting errors for '{}': {:?}", key, errors);
                }

                result.into_owned()
            })
            .unwrap_or_else(|| key.to_string())
    }

    /// Get localized string with HashMap args (for backward compatibility)
    pub fn get_with_args(&self, key: &str, args: Option<&HashMap<&str, &str>>) -> String {
        let mut errors = vec![];
        let bundle = self.current_bundle();

        let value = bundle
            .get_message(key)
            .and_then(|msg| msg.value())
            .map(|pattern| {
                let fluent_args = args.map(|a| {
                    a.iter()
                        .map(|(k, v)| (k.to_string(), FluentValue::from(*v)))
                        .collect()
                });
                bundle
                    .format_pattern(pattern, fluent_args.as_ref(), &mut errors)
                    .into_owned()
            })
            .unwrap_or_else(|| key.to_string());

        if !errors.is_empty() {
            eprintln!("Fluent formatting errors for '{}': {:?}", key, errors);
        }

        value
    }

    /// Check if current locale is Chinese
    pub fn is_chinese(&self) -> bool { self.current_locale.starts_with("zh") }
}

impl Default for I18nManager {
    fn default() -> Self { Self::new() }
}

// Macros for easy usage
#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::i18n::L10N.read().get($key)
    };
}

/// Macro with HashMap args (backward compatible)
#[macro_export]
macro_rules! t_args {
    ($key:expr, $($arg_key:expr => $arg_val:expr),+ $(,)?) => {{
        let mut args = fluent_bundle::FluentArgs::new();
        $(
            args.set($arg_key, $arg_val);
        )+
        $crate::i18n::L10N.read().get_with_fluent_args($key, Some(&args))
    }};
}

/// Macro with FluentArgs (type-safe, supports numbers, etc.)
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
        assert!(AVAILABLE_LOCALES.contains(&manager.current_locale()));
    }

    #[test]
    fn test_get_message() {
        let mut manager = I18nManager::new();
        manager.set_locale("en-US");
        assert_eq!(manager.get("app-title"), "SAide");
    }

    #[test]
    fn test_get_with_args() {
        let mut manager = I18nManager::new();
        manager.set_locale("en-US");
        let mut args = HashMap::new();
        args.insert("backend", "Vulkan");
        let msg = manager.get_with_args("config-video-backend", Some(&args));
        assert!(msg.contains("Vulkan"));
    }

    #[test]
    fn test_get_with_fluent_args() {
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
    fn test_macro_t_args() {
        let greeting = t_args!("config-video-backend", "backend" => "Vulkan");
        assert!(greeting.contains("Vulkan"));
    }

    #[test]
    fn test_macro_tf_with_number() {
        let msg = tf!("config-max-fps", "fps" => 60);
        assert!(msg.contains("60"));
    }

    #[test]
    fn test_locale_switch_performance() {
        let mut manager = I18nManager::new();

        // Should be instant (no re-parsing, just pointer switch)
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
}
