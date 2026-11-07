//! The vendored winit's primary-button dedupe, tested. The code lives in
//! `third_party/winit/src/platform_impl/android/primary_button.rs`, a module winit
//! compiles only on Android (`#[cfg(android_platform)]`), and `cargo test -p winit`
//! is closed to this workspace (a non-member patched package cannot build its
//! dev-dependencies) — so the file is included here to run its tests on every host,
//! under the ordinary `cargo test -p deckcraft-ui-egui` gate.
//!
//! Why the dedupe exists: Android reports a mouse's primary button twice per
//! transition (as `Down`/`Up` and as `ButtonPress`/`ButtonRelease`), so without it
//! one physical click reaches egui as two — and a single click selects a word.

include!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../third_party/winit/src/platform_impl/android/primary_button.rs"));
