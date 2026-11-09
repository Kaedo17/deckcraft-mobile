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
//! egui's text (rfd and arboard have no backend for the target): Open, Save As and export are
//! served by DeckCraft's own in-app file browser over the app-private storage, reaching shared
//! storage only while Android's "All files access" grant holds (`android_access`), and there is
//! no per-user settings folder (`HOME` and `XDG_CONFIG_HOME` are unset): preferences, the
//! log file and AutoRecover copies live in the app-private storage directory the activity resolves
//! (`android_storage` in the app library). Spell-check dictionaries and system fonts have no
//! Android location either, so those degrade to their built-in defaults.
//!
//! On every other target this crate is empty: only Android's loader ever looks for `android_main`.

#[cfg(target_os = "android")]
mod android_access;

/// NativeActivity's main-loop thread entry point (required; see the `android-activity` docs).
/// winit pumps the activity's lifecycle and input events inside [`deckcraft::run_android`], which
/// needs the activity's handle to create its event loop; when the activity is destroyed that
/// returns and this returns with it.
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
pub fn android_main(app: android_activity::AndroidApp) {
    // The "All files access" grant, checked and requested over JNI, and Back's record bound to the
    // activity's class over JNI — this crate's one unsafe stays in `android_access`; the app only
    // ever sees the plain closures built here.
    let access = android_access::capture(&app);
    // Bind before the loop starts, so no gesture can arrive against an unbound method. Android's
    // own JNI name lookup cannot find this library from the app's class loader, so it is registered
    // explicitly; if that fails the activity catches the call rather than crashing the presentation.
    if let Some(a) = access {
        android_access::register_back(a);
    }
    let shared = access.map(|a| deckcraft::SharedStorageAccess {
        granted: std::sync::Arc::new(move || a.shared_storage_granted()),
        ask: std::sync::Arc::new(move || {
            a.ask_shared_storage_grant();
        }),
    });
    // Android's Back, recorded by DeckCraft's activity's back callback (`android_access::record_back`
    // over JNI): Android answers Back itself before any key event reaches the app, so the activity
    // consumes it and notes it here; the app answers Escape, which dismisses a dialog, a menu or a
    // cancellation, and the activity stays foreground. Home still leaves.
    let back = std::sync::Arc::new(android_access::take_back as fn() -> bool);
    if let Err(e) = deckcraft::run_android(app, shared, Some(back)) {
        log::error!("deckcraft exited with an error: {e}");
    }
}
