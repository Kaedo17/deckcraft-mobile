//! DeckCraft's app-private Android storage. Android gives an app one writable directory (the
//! activity's internal data path); `HOME` and `XDG_CONFIG_HOME` are unset, so preferences, the log
//! file, AutoRecover copies and the in-app browser's Documents folder all hang off that one base.
//! The desktop resolves its settings folder from the environment instead (see [`crate::config_dir`]).
//!
//! Resolved once, before anything reads it: an Android launch records the activity's internal data
//! directory in [`set`] at the top of `run_inner`. Every folder helper creates the folder it names
//! and returns `None` when storage is unresolved or creation fails — callers degrade exactly as
//! they do on a desktop with no settings directory.

use std::path::PathBuf;
use std::sync::OnceLock;

static BASE: OnceLock<PathBuf> = OnceLock::new();

/// Record the activity's internal data directory. Called once per Android launch before any read;
/// a later call keeps the first base.
pub fn set(base: PathBuf) {
    let _ = BASE.set(base);
}

/// The recorded base, if this launch resolved one.
pub fn base() -> Option<PathBuf> {
    BASE.get().cloned()
}

/// A named folder under the base, created if missing; `None` when unresolved or creation fails.
fn dir(sub: &str) -> Option<PathBuf> {
    let path = base()?.join(sub);
    match std::fs::create_dir_all(&path) {
        Ok(()) => Some(path),
        Err(e) => {
            log::warn!("app-private storage: {sub}: {e}");
            None
        }
    }
}

/// The settings folder: `ui.json`, `prefs.json` and `logs/` — the Android answer to the desktop's
/// settings directory, so `settings::load`/`save` and the log file need no Android branch of their
/// own. One resolution serves both this and the file browser (a later step), which shares `base`.
pub fn config() -> Option<PathBuf> {
    dir("config")
}

/// Presentations the app offers to open and save: the in-app file browser starts here (spec C).
/// The name matches the browser's start folder (`ui-egui`'s `app_start`).
#[cfg_attr(not(target_os = "android"), allow(dead_code))] // android creates it at launch; desktop hosts don't list it
pub fn documents() -> Option<PathBuf> {
    dir("Documents")
}

/// AutoRecover copies, beside the desktop's `<settings dir>/Recovery`.
pub fn recovery() -> Option<PathBuf> {
    dir("recovery")
}
