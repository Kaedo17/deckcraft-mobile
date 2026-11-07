//! DeckCraft desktop app binary: a shim over the app library (`lib.rs`), whose documentation
//! carries the usage. [`deckcraft::run`] opens the window and runs the event loop.
#![cfg_attr(all(target_os = "windows", not(debug_assertions)), windows_subsystem = "windows")]

fn main() -> eframe::Result {
    deckcraft::run()
}
