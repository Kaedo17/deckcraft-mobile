# Vendored winit (Android input delta)

This is the `winit` that DeckCraft's workspace already pinned:
`storytold/winit` rev `a517b3846ee8457d84ee4975110bfda6797c44dc` — the
egui-0.37-compatible fork eframe 0.37 builds against — unmodified except for
`src/platform_impl/android/mod.rs` and the `primary_button.rs` module beside
it, vendored because DeckCraft's Android build
has no upstream route to the fix: the fork carries upstream's `//
TODO mouse events` in the Android backend (0.31 is still in beta), so a mouse
attached to the device (Bluetooth, USB or DeX) gets no hover, no scroll wheel
and no right or middle button; only touch works.

The workspace pins this copy with `[patch.crates-io]` in the root `Cargo.toml`;
nothing else in the tree may reference it directly.

## The delta (two files: `android/mod.rs` and `android/primary_button.rs`)

Upstream's Android backend translates every `MotionEvent` into a `Touch` event
and drops the rest, so a mouse attached to the device (Bluetooth, USB or DeX) has
no hover, no scroll wheel and no right or middle button; only touch works. The
patch adds one method, `EventLoop::dispatch_mouse_input`, and one call to it:

- pointers whose `Source` is `Mouse` or `Touchpad` are routed to the new method
  (touchscreen and touch-dragged stylus pointers keep the untouched `Touch` path);
- `Scroll` becomes `MouseWheel { delta: LineDelta(h, v) }` from the event's
  `Vscroll`/`Hscroll` axes;
- `ButtonPress`/`ButtonRelease` become `MouseInput` for the button whose bit the
  button state gained or lost since the previous button event (`EventLoop`'s
  `buttons_down`) — right, middle, back, forward, and primary if a device sends
  it that way — preceded by a `CursorMoved`;
- a mouse's primary button arrives as an ordinary `Down`/`Up` (per Android's
  button-event rules), which become `MouseInput` for `MouseButton::Left` —
  while a non-primary bare `Down` is swallowed, so a right click does not also
  select as a left one;
- `Move`/`HoverMove` become `CursorMoved`, `HoverEnter`/`HoverExit` become
  `CursorEntered`/`CursorLeft`, so the cursor tracks the mouse without a press.

Devices also report the primary button through *both* pairs — `Down`/`Up` and
`ButtonPress`/`ButtonRelease` carry it — so without a dedupe one physical click
reaches egui as two, and egui's double-click counter turns a single click into
a word selection. `android/primary_button.rs` (`EventLoop`'s `primary_button`)
reports each transition once: a repeat of the already-reported state is
swallowed, a genuine rapid second click still reports, and `Cancel` (a gesture
that ends without its final transition) resets the latch so the next click's
press is never dropped. Its unit tests run on every host through
`crates/ui-egui/tests/android_primary_button.rs`, which includes the file
verbatim (`cargo test -p winit` is closed to this workspace: a non-member
patched package cannot build its dev-dependencies).

The changed button comes from the button-state diff, not `action_button()`: a
Huawei MediaPad (API 31) whose libandroid omits `AMotionEvent_getActionButton`
(an API 33 addition some builds omit) failed to load the app — even a reference
to the symbol fails the whole library with an `UnsatisfiedLinkError`. With the
state diff the API 33 symbol is gone from the backend, so any Android version
loads.

The delta (the routing, the method and its follow-up fixes) is FilmCraft's, from
the same port series in `filmcraft`'s `third_party/winit` — the method differs
only in the base it documents; the dedupe and the button-state derivation are
WordCraft's, copied here (not depended on) as the house rules allow.

Keyboard events are **not** touched here: upstream also never emits
`ModifiersChanged` on Android (it ignores the meta state Android carries on every
`KeyEvent`), but DeckCraft fixes that on its own side, in `crates/ui-egui`, where
it is unit-testable — see `stamp_android_modifiers` in `crates/ui-egui/src/app/input.rs`.

## Rebasing onto a newer winit

1. Take the new pinned revision as the base again (replace this directory,
   keeping this README), then drop the images it ships for its own docs and
   examples (`rm docs/res/*.svg examples/data/*.png`): they are never compiled
   or shipped, and DeckCraft's `cargo xtask assets` gate attributes every asset
   in the repository file by file. Drop the checkout's `.git` and `.cargo-ok`
   too — this is a source snapshot, not a clone.
2. Re-apply the delta above; it is self-contained in the `MotionEvent` arm of
   `handle_input_event` plus the new method beside it, with `primary_button.rs`
   and the `EventLoop` fields it hangs off. Search for
   `TODO mouse events` — as long as upstream still has it, the hunk applies in
   the same place.
3. If upstream has implemented Android mouse input, delete this directory, drop
   the `[patch.crates-io]` entry, and delete `stamp_android_modifiers` in
   `crates/ui-egui/src/app/input.rs` only after checking that the new `winit` emits
   `ModifiersChanged` there too.

Licensed Apache-2.0, as upstream; `LICENSE` is upstream's, and every other file
is the pinned revision's own, untouched; the only source changes are the two
Android input files.
