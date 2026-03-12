// SPDX-License-Identifier: MIT OR Apache-2.0

//! Internationalization manager using Fluent.
//!
//! This module provides a thread-safe internationalization (i18n) manager that
//! utilizes the Fluent localization system. It supports loading translation
//! resources from either the filesystem (in debug mode) or embedded resources
//! (in release mode). The manager automatically detects the system locale,
//! allows switching between available locales, and provides methods to retrieve
//! localized messages with optional Fluent arguments.
//!
//! ## Performance Optimization
//!
//! To avoid repeated lookups and formatting during egui's frequent redraws,
//! this module implements a thread-local LRU cache for translation results.
//! The cache is automatically invalidated when the locale changes.

use {
    fluent_bundle::{FluentArgs, FluentResource, bundle::FluentBundle},
    intl_memoizer::concurrent::IntlLangMemoizer,
    lru::LruCache,
    once_cell::sync::Lazy,
    parking_lot::RwLock,
    std::{
        cell::RefCell,
        collections::HashMap,
        num::NonZeroUsize,
        sync::{
            Arc,
            atomic::{AtomicU64, Ordering},
        },
    },
    unic_langid::LanguageIdentifier,
};

type ThreadSafeFluentBundle = FluentBundle<FluentResource, IntlLangMemoizer>;

/// Maximum number of cached translations per thread
const CACHE_CAPACITY: usize = 256;

/// Global generation counter for cache invalidation
static CACHE_GENERATION: AtomicU64 = AtomicU64::new(0);

// Thread-local cache for translation results
thread_local! {
    static TRANSLATION_CACHE: RefCell<TranslationCache> = RefCell::new(TranslationCache::new());
}

/// Translation cache with generation counter for invalidation
struct TranslationCache {
    cache: LruCache<String, String>,
    generation: u64,
}

impl TranslationCache {
    fn new() -> Self {
        Self {
            cache: LruCache::new(NonZeroUsize::new(CACHE_CAPACITY).unwrap()),
            generation: 0,
        }
    }

    fn get(&mut self, key: &str, current_gen: u64) -> Option<String> {
        if self.generation != current_gen {
            self.cache.clear();
            self.generation = current_gen;
            return None;
        }
        self.cache.get(key).cloned()
    }

    fn insert(&mut self, key: String, value: String, current_gen: u64) {
        if self.generation != current_gen {
            self.cache.clear();
            self.generation = current_gen;
        }
        self.cache.put(key, value);
    }
}

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
            if key.starts_with(prefix) {
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

    fn current_bundle(&self) -> Option<&ThreadSafeFluentBundle> {
        self.bundles
            .get(&self.current_locale)
            .or_else(|| self.bundles.values().next())
    }

    pub fn current_locale(&self) -> &str { &self.current_locale }

    pub fn set_locale(&mut self, locale: &str) {
        let normalized = Self::normalize_locale(locale);

        if self.bundles.contains_key(&normalized) {
            self.current_locale = normalized;
            self.invalidate_cache();
            return;
        }

        let prefix = normalized.split('-').next().unwrap_or("en");
        for key in self.bundles.keys() {
            if key.starts_with(prefix) {
                self.current_locale = key.clone();
                self.invalidate_cache();
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
                self.invalidate_cache();
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

    fn invalidate_cache(&self) { CACHE_GENERATION.fetch_add(1, Ordering::Relaxed); }

    fn current_generation(&self) -> u64 { CACHE_GENERATION.load(Ordering::Relaxed) }

    pub fn get(&self, key: &str) -> String { self.get_with_fluent_args(key, None) }

    pub fn get_with_fluent_args(&self, key: &str, args: Option<&FluentArgs>) -> String {
        let cache_key = if let Some(args) = args {
            format!("{}:{:?}", key, args)
        } else {
            key.to_string()
        };

        let current_gen = self.current_generation();

        if let Some(cached) =
            TRANSLATION_CACHE.with(|cache| cache.borrow_mut().get(&cache_key, current_gen))
        {
            return cached;
        }

        if let Some(bundle) = self.current_bundle() {
            let mut errors = vec![];
            let value = bundle
                .get_message(key)
                .and_then(|msg| msg.value())
                .map(|pattern| {
                    let result = bundle.format_pattern(pattern, args, &mut errors);
                    if !errors.is_empty() {
                        tracing::debug!("Fluent formatting errors for '{}': {:?}", key, errors);
                    }
                    result.into_owned()
                })
                .unwrap_or_else(|| key.to_string());

            TRANSLATION_CACHE.with(|cache| {
                cache
                    .borrow_mut()
                    .insert(cache_key, value.clone(), current_gen);
            });

            value
        } else {
            tracing::warn!("No i18n bundle loaded, returning key as-is: {}", key);
            key.to_string()
        }
    }

    pub fn is_chinese(&self) -> bool { self.current_locale.starts_with("zh") }
}

pub fn current_generation() -> u64 { CACHE_GENERATION.load(Ordering::Relaxed) }

pub fn get_cached(key: &str) -> Option<String> {
    let generation = current_generation();
    TRANSLATION_CACHE.with(|cache| cache.borrow_mut().get(key, generation))
}

pub fn set_cached(key: String, value: String) {
    let generation = current_generation();
    TRANSLATION_CACHE.with(|cache| {
        cache.borrow_mut().insert(key, value, generation);
    });
}

impl Default for I18nManager {
    fn default() -> Self { Self::new() }
}

pub static L10N: Lazy<Arc<RwLock<I18nManager>>> =
    Lazy::new(|| Arc::new(RwLock::new(I18nManager::new())));
