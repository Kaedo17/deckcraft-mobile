#![allow(clippy::unwrap_used, clippy::expect_used)]

//! The Android modifier stamp. winit's Android backend never emits `ModifiersChanged` (it
//! ignores the meta state Android carries on every key event), so egui-winit's modifiers state
//! is always empty there: a Ctrl+S chord reads as a bare S, a Ctrl+wheel as a plain wheel and a
//! Ctrl+click as a plain click. `super::input::stamp_android_modifiers` tracks the modifier key
//! events, fills every other event's modifiers with what was held when it fired, and ends the
//! frame with a `ModifiersChanged` carrying the end state.

use egui::{Event, Key, Modifiers};

fn key(key: Key, pressed: bool) -> Event {
    Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers: Modifiers::NONE }
}

fn stamp(events: Vec<Event>) -> (Vec<Event>, Modifiers) {
    let mut events = events;
    let mut held = Modifiers::NONE;
    super::input::stamp_android_modifiers(&mut events, &mut held);
    (events, held)
}

/// The modifiers each key event of the frame carries, in order.
fn stamped(events: &[Event]) -> Vec<Modifiers> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Key { modifiers, .. } => Some(*modifiers),
            _ => None,
        })
        .collect()
}

#[test]
fn ctrl_chord_gets_the_command_modifiers() {
    let (events, held) = stamp(vec![key(Key::ControlLeft, true), key(Key::S, true)]);
    // The Ctrl press itself is left alone; the S press under it carries Ctrl…
    assert_eq!(stamped(&events), vec![Modifiers::NONE, Modifiers { ctrl: true, command: true, ..Modifiers::NONE }]);
    assert!(held.ctrl && held.command);
}

#[test]
fn a_released_ctrl_ends_the_chord_within_the_frame() {
    let ctrl = Modifiers { ctrl: true, command: true, ..Modifiers::NONE };
    let (events, held) =
        stamp(vec![key(Key::Z, true), key(Key::ControlLeft, true), key(Key::Z, true), key(Key::ControlRight, false), key(Key::Z, true)]);
    // Five key events, none of the modifier presses/releases stamped: the Z under Ctrl
    // carries the chord, the Z after the release does not.
    assert_eq!(stamped(&events), vec![Modifiers::NONE, Modifiers::NONE, ctrl, Modifiers::NONE, Modifiers::NONE]);
    assert_eq!(held, Modifiers::NONE);
}

#[test]
fn shift_and_alt_are_tracked() {
    let both = Modifiers { shift: true, alt: true, ..Modifiers::NONE };
    let (events, held) = stamp(vec![key(Key::ShiftLeft, true), key(Key::AltRight, true), key(Key::F5, true)]);
    assert_eq!(stamped(&events)[2], both);
    assert_eq!(held, both);
}

#[test]
fn a_lost_focus_clears_a_stuck_modifier() {
    let (events, held) = stamp(vec![key(Key::ControlLeft, true), Event::WindowFocused(false), key(Key::S, true)]);
    // stamped() only collects key events: the Ctrl press, then the S press after the focus loss.
    assert_eq!(stamped(&events)[1], Modifiers::NONE, "no chord may survive a focus loss");
    assert_eq!(held, Modifiers::NONE);
}

#[test]
fn the_frame_announces_its_end_state() {
    let (events, _) = stamp(vec![key(Key::ControlLeft, true)]);
    assert!(matches!(events.last(), Some(Event::ModifiersChanged(m)) if m.ctrl && m.command));
    let (events, _) = stamp(vec![]);
    assert!(matches!(events.last(), Some(Event::ModifiersChanged(m)) if *m == Modifiers::NONE));
}

#[test]
fn stamped_modifiers_match_the_binding_patterns() {
    // What the shortcut pass then does with them: Ctrl+S matches a COMMAND binding and not a
    // plain-key one, and an unmodified key the other way around. DeckCraft's chords run through
    // `Session::key`, which matches on the event's own modifiers.
    let (events, _) = stamp(vec![key(Key::ControlLeft, true), key(Key::S, true)]);
    let chord = stamped(&events)[1];
    assert!(chord.matches_logically(Modifiers::COMMAND));
    assert!(!chord.matches_logically(Modifiers::NONE));
    let (events, _) = stamp(vec![key(Key::Z, true)]);
    let plain = stamped(&events)[0];
    assert!(plain.matches_logically(Modifiers::NONE));
    assert!(!plain.matches_logically(Modifiers::COMMAND));
}

fn wheel_event(delta: egui::Vec2, modifiers: Modifiers) -> Event {
    Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta, phase: egui::TouchPhase::Move, source: egui::MouseWheelSource::Unknown, modifiers }
}

fn click_with(m: Modifiers) -> Event {
    Event::PointerButton { pos: egui::pos2(10.0, 10.0), button: egui::PointerButton::Primary, pressed: true, modifiers: m }
}

#[test]
fn wheel_and_click_carry_the_held_modifiers() {
    let ctrl = Modifiers { ctrl: true, command: true, ..Modifiers::NONE };
    let (events, _) = stamp(vec![key(Key::ControlLeft, true), wheel_event(egui::vec2(0.0, 1.0), Modifiers::NONE), click_with(Modifiers::NONE)]);
    // Ctrl + wheel zoom and Ctrl + click read the event's own modifiers, never the frame's.
    assert_eq!(events[1], wheel_event(egui::vec2(0.0, 1.0), ctrl), "the wheel under Ctrl carries the chord");
    assert_eq!(events[2], click_with(ctrl), "a click under Ctrl carries the chord");
}
