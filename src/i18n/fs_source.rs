//! Filesystem-based FTL source with file watching capabilities.
//!
//! This module provides an implementation of the `FtlSource` trait that loads
//! Fluent translation resources from the filesystem. In debug mode, it also
//! supports hot reloading of FTL files by watching for file changes.

use {
    fluent_bundle::FluentResource,
    notify::{RecommendedWatcher, RecursiveMode, Watcher, event::Event},
    std::{
        fs,
        path::{Path, PathBuf},
        sync::mpsc,
        thread,
    },
    tracing,
};

pub trait FtlWatcherSource {
    fn reload_locale(&mut self, locale: &str) -> Result<Vec<FluentResource>, String>;
}

pub struct FsFtlSource {
    root: PathBuf,
    watcher: Option<RecommendedWatcher>,
    watcher_tx: Option<mpsc::Sender<Result<Event, notify::Error>>>,
}

impl FsFtlSource {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            watcher: None,
            watcher_tx: None,
        }
    }

    pub(crate) fn find_i18n_path() -> PathBuf {
        let current_exe = std::env::current_exe().unwrap_or_else(|e| {
            tracing::warn!("Failed to get current executable path: {e}, using '.'");
            PathBuf::from(".")
        });
        let current_dir = current_exe.parent().unwrap_or_else(|| {
            tracing::warn!("Executable has no parent directory, using '.'");
            Path::new(".")
        });

        let paths = [
            current_dir.join("i18n"),
            PathBuf::from("i18n"),
            PathBuf::from("../i18n"),
        ];

        for path in &paths {
            if path.exists() && path.is_dir() {
                return path.to_path_buf();
            }
        }

        tracing::warn!("i18n directory not found, falling back to './i18n'");
        PathBuf::from("i18n")
    }
}

impl FtlWatcherSource for FsFtlSource {
    fn reload_locale(&mut self, locale: &str) -> Result<Vec<FluentResource>, String> {
        super::FtlSource::load_locale(self, locale)
    }
}

impl super::FtlSource for FsFtlSource {
    fn available_locales(&self) -> Vec<String> {
        let mut locales = Vec::new();

        if !self.root.exists() {
            tracing::warn!("i18n directory not found: {:?}", self.root);
            return locales;
        }

        if let Ok(entries) = fs::read_dir(&self.root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir()
                    && let Some(name) = path.file_name().and_then(|n| n.to_str())
                    && Self::is_valid_locale_name(name)
                {
                    locales.push(name.to_string());
                }
            }
        }

        locales.sort();
        locales
    }

    fn load_locale(&self, locale: &str) -> Result<Vec<FluentResource>, String> {
        let dir = self.root.join(locale);

        if !dir.exists() {
            return Err(format!("Locale directory not found: {}", locale));
        }

        let mut resources = Vec::new();

        let entries = fs::read_dir(&dir).map_err(|e| e.to_string())?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("ftl") {
                let content = fs::read_to_string(&path).map_err(|e| e.to_string())?;
                let resource = FluentResource::try_new(content)
                    .map_err(|e| format!("Failed to parse FTL {:?}: {:?}", path, e))?;
                resources.push(resource);
            }
        }

        if resources.is_empty() {
            return Err(format!("No FTL files found for locale: {}", locale));
        }

        Ok(resources)
    }

    #[cfg(debug_assertions)]
    fn as_watcher_source(&mut self) -> Option<&mut dyn FtlWatcherSource> { Some(self) }
}

impl FsFtlSource {
    pub fn start_watcher(&mut self) {
        let (tx, rx) = mpsc::channel();
        let root = self.root.clone();
        let tx_clone = tx.clone();

        let mut watcher = match RecommendedWatcher::new(tx, notify::Config::default()) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!("i18n hot reload disabled (failed to create watcher): {e}");
                return;
            }
        };

        if let Err(e) = watcher.watch(&self.root, RecursiveMode::NonRecursive) {
            tracing::warn!("Failed to start i18n file watcher: {}", e);
            return;
        }

        self.watcher = Some(watcher);
        self.watcher_tx = Some(tx_clone);

        tracing::info!("Started i18n file watcher on: {:?}", self.root);

        thread::spawn(move || {
            Self::watcher_loop(rx, root);
        });
    }

    fn is_valid_locale_name(name: &str) -> bool { !name.starts_with('.') && !name.starts_with('_') }

    fn watcher_loop(rx: mpsc::Receiver<Result<Event, notify::Error>>, _root: PathBuf) {
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
                            if path.extension().and_then(|e| e.to_str()) == Some("ftl") {
                                tracing::info!("FTL file changed: {:?} - reloading", path);

                                if let Some(locale) = path
                                    .parent()
                                    .and_then(|p| p.file_name())
                                    .and_then(|n| n.to_str())
                                {
                                    super::L10N.write().reload_locale(locale);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("File watcher error: {}", e);
                }
            }
        }
    }
}
