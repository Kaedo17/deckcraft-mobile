//! Android entry point for DeckCraft.
//!
//! Android's `NativeActivity` loads this library and calls [`android_main`] on a dedicated
//! thread. It opens the shared app ([`deckcraft::run_android`], the same code the desktop binary
//! runs) and returns when the window closes, ending the activity's main-loop thread.
//!
//! This crate exists because that export must be unmangled, and on edition 2024 the only spelling
//! (`#[unsafe(no_mangle)]`) trips the workspace-forbidden `unsafe_code` lint — see `Cargo.toml`
//! for why the crate carries its own lints instead.
//!
//! Android limitations (v1): no command line, so the launch is the default one — no control
//! server, no `--sample`, no files to open. No system file dialogs and no system clipboard beyond
//! egui's text (rfd and arboard have no backend for the target), so Open, Save As and export
//! answer cancelled until DeckCraft's own in-app file browser serves them (a later step), and
//! there is no per-user settings folder (`HOME` and `XDG_CONFIG_HOME` are unset): preferences, the
//! log file and AutoRecover copies live in the app-private storage directory the activity resolves
//! (`android_storage` in the app library). Spell-check dictionaries and system fonts have no
//! Android location either, so those degrade to their built-in defaults.
//!
//! On every other target this crate is empty: only Android's loader ever looks for `android_main`.

/// NativeActivity's main-loop thread entry point (required; see the `android-activity` docs).
/// winit pumps the activity's lifecycle and input events inside [`deckcraft::run_android`], which
/// needs the activity's handle to create its event loop; when the activity is destroyed that
/// returns and this returns with it.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub fn android_main(app: android_activity::AndroidApp) {
    if let Err(e) = deckcraft::run_android(app) {
        log::error!("deckcraft exited with an error: {e}");
    }
}
