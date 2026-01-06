//! Thread-safe i18n manager with Fluent
//!
//! Features:
//! - Auto-scan i18n folder for available locales
//! - Support multiple FTL files per locale (auto-merged)
//! - Hot reload: watch FTL files and auto-reload bundles in debug mode

#[cfg(debug_assertions)]
use notify::{RecommendedWatcher, RecursiveMode, Watcher, event::Event};
use {
    crate::constant::DATA_PATH,
    fluent_bundle::{FluentArgs, FluentResource, bundle::FluentBundle},
    intl_memoizer::concurrent::IntlLangMemoizer,
    once_cell::sync::Lazy,
    parking_lot::RwLock,
    std::{
        collections::HashMap,
        fs,
        path::{Path, PathBuf},
        sync::{Arc, mpsc},
        thread,
    },
    tracing::{debug, error, info, warn},
    unic_langid::LanguageIdentifier,
};

const I18N_DIR: &str = "i18n";
const FTL_EXTENSION: &str = "ftl";

/// Global i18n manager instance
pub static L10N: Lazy<Arc<RwLock<I18nManager>>> =
    Lazy::new(|| Arc::new(RwLock::new(I18nManager::new())));

type ThreadSafeFluentBundle = FluentBundle<FluentResource, IntlLangMemoizer>;

pub struct I18nManager {
    current_locale: String,
    bundles: HashMap<String, ThreadSafeFluentBundle>,

    #[cfg(debug_assertions)]
    watcher: Option<RecommendedWatcher>,

    i18n_path: PathBuf,
}

impl I18nManager {
    pub fn new() -> Self {
        let i18n_path = Self::find_i18n_path();

        let bundles = Self::scan_and_load_all(&i18n_path);

        let locale = Self::detect_locale(&bundles);

        let mut manager = Self {
            current_locale: locale,
            bundles,

            #[cfg(debug_assertions)]
            watcher: None,

            i18n_path,
        };

        // Start file watcher in debug mode for hot-reloading
        #[cfg(debug_assertions)]
        {
            manager.start_watcher();
        }

        manager
    }

    /// Find the i18n directory path
    /// Searches in DATA_PATH/i18n and ./i18n
    /// Returns the first valid path found, or defaults to ./i18n
    fn find_i18n_path() -> PathBuf {
        let paths: Vec<PathBuf> = vec![DATA_PATH.join(I18N_DIR), PathBuf::from(I18N_DIR)];

        for path in &paths {
            if path.exists() && path.is_dir() {
                return path.to_path_buf();
            }
        }

        PathBuf::from(I18N_DIR)
    }

    fn scan_and_load_all(i18n_path: &Path) -> HashMap<String, ThreadSafeFluentBundle> {
        let mut bundles = HashMap::new();

        if !i18n_path.exists() {
            warn!("i18n directory not found: {:?}", i18n_path);
            return bundles;
        }

        let entries = match fs::read_dir(i18n_path) {
            Ok(entries) => entries,
            Err(e) => {
                error!("Failed to read i18n directory: {}", e);
                return bundles;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            if let Some(locale_name) = path.file_name().and_then(|n| n.to_str())
                && Self::is_valid_locale_name(locale_name)
            {
                if let Ok(bundle) = Self::load_bundle_from_dir(&path, locale_name) {
                    bundles.insert(locale_name.to_string(), bundle);
                    debug!("Loaded locale: {}", locale_name);
                } else {
                    warn!("Failed to load locale: {}", locale_name);
                }
            }
        }

        if bundles.is_empty() {
            error!("No valid locales found in i18n directory");
        }

        bundles
    }

    fn is_valid_locale_name(name: &str) -> bool { !name.starts_with('.') && !name.starts_with('_') }

    fn load_bundle_from_dir(dir: &Path, locale: &str) -> Result<ThreadSafeFluentBundle, String> {
        let langid: LanguageIdentifier = locale
            .parse()
            .map_err(|_| format!("Invalid language identifier: {}", locale))?;

        let mut bundle = FluentBundle::new_concurrent(vec![langid]);

        let ftl_files = Self::find_ftl_files(dir);

        if ftl_files.is_empty() {
            return Err(format!("No FTL files found for locale: {}", locale));
        }

        for ftl_path in &ftl_files {
            match Self::load_ftl_file(ftl_path) {
                Ok(resource) => {
                    bundle
                        .add_resource(resource)
                        .map_err(|e| format!("Failed to add resource: {:?}", e))?;
                    debug!("Loaded FTL file: {:?}", ftl_path);
                }
                Err(e) => {
                    warn!("Failed to load FTL file {:?}: {}", ftl_path, e);
                }
            }
        }

        Ok(bundle)
    }

    fn find_ftl_files(dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file()
                    && let Some(ext) = path.extension().and_then(|e| e.to_str())
                    && ext.eq_ignore_ascii_case(FTL_EXTENSION)
                {
                    files.push(path);
                }
            }
        }

        files.sort();
        files
    }

    fn load_ftl_file(path: &Path) -> Result<FluentResource, String> {
        let content =
            fs::read_to_string(path).map_err(|e| format!("Failed to read FTL file: {}", e))?;

        FluentResource::try_new(content).map_err(|e| format!("Failed to parse FTL: {:?}", e))
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

    #[cfg(debug_assertions)]
    fn start_watcher(&mut self) {
        let (tx, rx) = mpsc::channel();
        let i18n_path = self.i18n_path.clone();

        let mut watcher = RecommendedWatcher::new(tx, notify::Config::default())
            .expect("Failed to create file watcher");

        if let Err(e) = watcher.watch(&i18n_path, RecursiveMode::NonRecursive) {
            warn!("Failed to start i18n file watcher: {}", e);
            return;
        }

        info!("Started i18n file watcher on: {:?}", i18n_path);

        self.watcher = Some(watcher);

        thread::spawn(move || {
            Self::watcher_loop(rx, i18n_path);
        });
    }

    #[cfg(debug_assertions)]
    fn watcher_loop(rx: mpsc::Receiver<Result<Event, notify::Error>>, i18n_path: PathBuf) {
        for result in rx.iter() {
            match result {
                Ok(Event { kind, paths, .. }) => {
                    let is_relevant = matches!(
                        kind,
                        notify::event::EventKind::Create(_)
                            | notify::event::EventKind::Modify(_)
                            | notify::event::EventKind::Remove(_)
                    );

                    if is_relevant {
                        for path in paths {
                            if let Some(ext) = path.extension().and_then(|e| e.to_str())
                                && ext.eq_ignore_ascii_case(FTL_EXTENSION)
                            {
                                info!("FTL file changed: {:?} - reloading", path);

                                if let Some(locale_name) = path
                                    .parent()
                                    .and_then(|p| p.file_name())
                                    .and_then(|n| n.to_str())
                                {
                                    let mut manager = L10N.write();
                                    Self::reload_bundle(&mut manager, locale_name, &i18n_path);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    warn!("File watcher error: {}", e);
                }
            }
        }
    }

    #[cfg(debug_assertions)]
    fn reload_bundle(manager: &mut I18nManager, locale: &str, i18n_path: &Path) {
        let locale_path = i18n_path.join(locale);

        if !locale_path.exists() {
            warn!("Locale directory no longer exists: {}", locale);
            return;
        }

        match Self::load_bundle_from_dir(&locale_path, locale) {
            Ok(new_bundle) => {
                manager.bundles.insert(locale.to_string(), new_bundle);
                info!("Reloaded locale: {}", locale);

                if manager.current_locale == locale {
                    info!("Current locale refreshed: {}", locale);
                }
            }
            Err(e) => {
                error!("Failed to reload locale {}: {}", locale, e);
            }
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

    pub fn available_locales(&self) -> Vec<&str> {
        self.bundles.keys().map(|s| s.as_str()).collect()
    }

    /// Get localized string by key
    pub fn get(&self, key: &str) -> String { self.get_with_fluent_args(key, None) }

    /// Get localized string by key with Fluent arguments
    pub fn get_with_fluent_args(&self, key: &str, args: Option<&FluentArgs>) -> String {
        let bundle = self.current_bundle();
        let mut errors = vec![];

        bundle
            .get_message(key)
            .and_then(|msg| msg.value())
            .map(|pattern| {
                let result = bundle.format_pattern(pattern, args, &mut errors);

                if !errors.is_empty() {
                    debug!("Fluent formatting errors for '{}': {:?}", key, errors);
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

/// Macro to get localized string by key
/// Usage: t!("key-name")
#[macro_export]
macro_rules! t {
    ($key:expr) => {
        $crate::i18n::L10N.read().get($key)
    };
}

/// Macro to get localized string by key with Fluent arguments
/// Usage: tf!("key-name", "arg1" => value1, "arg2" => value2)
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
