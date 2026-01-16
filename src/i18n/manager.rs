//! Internationalization manager using Fluent.
//!
//! This module provides a thread-safe internationalization (i18n) manager that
//! utilizes the Fluent localization system. It supports loading translation
//! resources from either the filesystem (in debug mode) or embedded resources
//! (in release mode). The manager automatically detects the system locale,
//! allows switching between available locales, and provides methods to retrieve
//! localized messages with optional Fluent arguments.

use {
    fluent_bundle::{FluentArgs, FluentResource, bundle::FluentBundle},
    intl_memoizer::concurrent::IntlLangMemoizer,
    once_cell::sync::Lazy,
    parking_lot::RwLock,
    std::{collections::HashMap, sync::Arc},
    unic_langid::LanguageIdentifier,
};

type ThreadSafeFluentBundle = FluentBundle<FluentResource, IntlLangMemoizer>;

pub struct I18nManager {
    source: Box<dyn super::FtlSource + Send + Sync>,
    bundles: HashMap<String, ThreadSafeFluentBundle>,
    current_locale: String,
}

impl I18nManager {
    pub fn new() -> Self {
        let source: Box<dyn super::FtlSource + Send + Sync> = {
            #[cfg(debug_assertions)]
            {
                let root = super::fs_source::FsFtlSource::find_i18n_path();
                let mut fs_source = super::fs_source::FsFtlSource::new(root);
                fs_source.start_watcher();
                Box::new(fs_source)
            }

            #[cfg(not(debug_assertions))]
            {
                Box::new(super::embedded::EmbeddedFtlSource::new())
            }
        };

        let mut bundles = HashMap::new();

        for locale in source.available_locales() {
            match source.load_locale(&locale) {
                Ok(resources) => {
                    let langid: LanguageIdentifier = locale.parse().unwrap_or_else(|e| {
                        tracing::warn!("Invalid locale '{locale}': {e}, falling back to en-US");
                        "en-US".parse().expect("en-US is always valid")
                    });
                    let mut bundle = FluentBundle::new_concurrent(vec![langid]);

                    for resource in resources {
                        bundle
                            .add_resource(resource)
                            .unwrap_or_else(|e| tracing::error!("Failed to add resource: {:?}", e));
                    }

                    bundles.insert(locale.clone(), bundle);
                    tracing::info!("Loaded locale: {}", locale);
                }
                Err(e) => {
                    tracing::warn!("Failed to load locale {}: {}", locale, e);
                }
            }
        }

        let current_locale = Self::detect_locale(&bundles);

        Self {
            source,
            bundles,
            current_locale,
        }
    }

    fn detect_locale(bundles: &HashMap<String, ThreadSafeFluentBundle>) -> String {
        if bundles.is_empty() {
            return "en-US".to_string();
        }

        let sys_locale = sys_locale::get_locale().unwrap_or_else(|| "en-US".to_string());
        let normalized = Self::normalize_locale(&sys_locale);

        if bundles.contains_key(&normalized) {
            return normalized;
        }

        let prefix = normalized.split('-').next().unwrap_or("en");

        for key in bundles.keys() {
            if key.starts_with(prefix) || key.starts_with(&normalized[..2]) {
                return key.clone();
            }
        }

        bundles
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| "en-US".to_string())
    }

    fn normalize_locale(locale: &str) -> String {
        match locale.split('-').next().unwrap_or("en") {
            "zh" => "zh-CN".to_string(),
            "en" => "en-US".to_string(),
            "ja" => "ja-JP".to_string(),
            "ko" => "ko-KR".to_string(),
            _ => locale.to_string(),
        }
    }

    fn current_bundle(&self) -> &ThreadSafeFluentBundle {
        self.bundles
            .get(&self.current_locale)
            .or_else(|| self.bundles.values().next())
            .expect("At least one bundle should exist")
    }

    pub fn current_locale(&self) -> &str { &self.current_locale }

    pub fn set_locale(&mut self, locale: &str) {
        let normalized = Self::normalize_locale(locale);

        if self.bundles.contains_key(&normalized) {
            self.current_locale = normalized;
            return;
        }

        for key in self.bundles.keys() {
            if key.starts_with(&normalized[..2]) {
                self.current_locale = key.clone();
                return;
            }
        }

        let prefix = normalized.split('-').next().unwrap_or("en");
        for key in self.bundles.keys() {
            if key.starts_with(prefix) {
                self.current_locale = key.clone();
                return;
            }
        }
    }

    pub fn reload_locale(&mut self, locale: &str) {
        if !self.bundles.contains_key(locale) {
            tracing::warn!("Locale not loaded, cannot reload: {}", locale);
            return;
        }

        match self.source.load_locale(locale) {
            Ok(resources) => {
                let langid: LanguageIdentifier = locale.parse().unwrap_or_else(|e| {
                    tracing::warn!("Invalid locale '{locale}': {e}, falling back to en-US");
                    "en-US".parse().expect("en-US is always valid")
                });
                let mut bundle = FluentBundle::new_concurrent(vec![langid]);

                for resource in resources {
                    bundle
                        .add_resource(resource)
                        .unwrap_or_else(|e| tracing::error!("Failed to add resource: {:?}", e));
                }

                self.bundles.insert(locale.to_string(), bundle);
                tracing::info!("Reloaded locale: {}", locale);
            }
            Err(e) => {
                tracing::error!("Failed to reload locale {}: {}", locale, e);
            }
        }
    }

    pub fn available_locales(&self) -> Vec<&str> {
        self.bundles.keys().map(|s| s.as_str()).collect()
    }

    pub fn get(&self, key: &str) -> String { self.get_with_fluent_args(key, None) }

    pub fn get_with_fluent_args(&self, key: &str, args: Option<&FluentArgs>) -> String {
        let bundle = self.current_bundle();
        let mut errors = vec![];

        bundle
            .get_message(key)
            .and_then(|msg| msg.value())
            .map(|pattern| {
                let result = bundle.format_pattern(pattern, args, &mut errors);

                if !errors.is_empty() {
                    tracing::debug!("Fluent formatting errors for '{}': {:?}", key, errors);
                }

                result.into_owned()
            })
            .unwrap_or_else(|| key.to_string())
    }

    pub fn is_chinese(&self) -> bool { self.current_locale.starts_with("zh") }
}

impl Default for I18nManager {
    fn default() -> Self { Self::new() }
}

pub static L10N: Lazy<Arc<RwLock<I18nManager>>> =
    Lazy::new(|| Arc::new(RwLock::new(I18nManager::new())));
