//! Finger-press arbitration for the canvas: a finger drag on an empty slide pans the zoomed
//! slide stack instead of growing a marquee selection; a quick release stays a click (the
//! engine's own Down/Up answers it); a press that lands on a shape, caret or handle is the
//! engine's and is never arbitrated; a still hold past the arbiter's expiry lets the press grow
//! a selection as a desktop drag does, and egui's long-press opens the canvas menu. Mouse presses
//! are never arbitrated, so desktop behaviour is untouched.

use egui::{Id, Pos2, Response, Ui, Vec2};

/// How far a finger may wobble before the press stops being a click (egui's `max_click_dist`).
const MOVED: f32 = 6.0;
/// The hold a finger keeps before a move grows a selection instead of panning. Below egui's own
/// long-press timeout (0.8 s), so the two paths never race.
const HOLD: f64 = 0.55;

/// What the finger's press turned out to mean this frame. Latched once, so a pan never turns
/// into a selection-growth because the finger later travels past the hold, and vice versa.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum TouchVerdict {
    /// No finger press this frame (a mouse press, or none): the canvas answers as always.
    None,
    /// A finger press neither moved far enough nor held long enough yet: keep waiting.
    Pressing,
    /// The finger moved before the hold expired: pan the canvas by `delta` and let the engine's
    /// marquee stand down for the rest of the press.
    Panning { delta: Vec2 },
    /// The engine owns the press (a shape, caret, vertex, handle), or the hold expired and the
    /// press may grow a selection the way a desktop drag does.
    Owned,
}

#[derive(Clone, Copy, Debug)]
struct Press {
    origin: Pos2,
    /// The engine's latch said owned on the frame after the press: shape presses stand down at
    /// once. Latched, so a later frame cannot reopen the arbitration.
    owned: bool,
    /// Panned before the hold: latched for the rest of the press.
    panning: bool,
}

fn press_id() -> Id {
    Id::unique("deckcraft-canvas-touch-press")
}

/// The canvas consults this every frame before sending pointer events and before applying scroll
/// offsets. `engine_owned` is whether the engine latched a non-marquee press (`ToolState::
/// dragging() && !marquee_press()`): those presses are the engine's own and stand the arbiter
/// down.
pub(super) fn verdict(ui: &Ui, resp: &Response, engine_owned: bool) -> TouchVerdict {
    let ctx = ui.ctx().clone();
    let (touch, pressed, down, origin, start, pos, delta, time) = ctx.input(|i| {
        (
            i.any_touches(),
            i.pointer.primary_pressed(),
            i.pointer.any_down(),
            i.pointer.press_origin(),
            i.pointer.press_start_time(),
            i.pointer.interact_pos().or(i.pointer.hover_pos()),
            i.pointer.delta(),
            i.time,
        )
    });
    let press = ctx.data(|d| d.get_temp::<Option<Press>>(press_id())).flatten();
    let press = match press {
        Some(p) => Some(p),
        None => {
            let origin = origin.filter(|o| touch && pressed && resp.rect.contains(*o));
            let p = origin.map(|o| Press { origin: o, owned: false, panning: false });
            if p.is_some() {
                ctx.data_mut(|d| d.insert_temp(press_id(), p));
            }
            p
        }
    };
    let Some(press) = press else { return TouchVerdict::None };

    if !down {
        // The finger lifted: the engine answers the release (or the click) itself.
        ctx.data_mut(|d| d.remove::<Option<Press>>(press_id()));
        return TouchVerdict::None;
    }
    if !touch {
        // A mouse press while a finger press was latched (or the touch state was lost): the
        // arbiter adds nothing this frame.
        return TouchVerdict::None;
    }
    let Some(pos) = pos else {
        // A `PointerGone` while held (a touch release is such a frame): keep waiting.
        return TouchVerdict::Pressing;
    };

    if engine_owned {
        ctx.data_mut(|d| d.insert_temp(press_id(), Some(Press { owned: true, ..press })));
        return TouchVerdict::Owned;
    }
    if press.panning {
        // Panning stays panning for the rest of the press, whatever the finger does next.
        return TouchVerdict::Panning { delta };
    }
    if press.owned || press_start_held(start, time) {
        // The hold expired with an empty press: a move after it grows the selection as a desktop
        // drag does. Latched, so the press cannot fall back into a pan.
        ctx.data_mut(|d| d.insert_temp(press_id(), Some(Press { owned: true, ..press })));
        return TouchVerdict::Owned;
    }
    if press.origin.distance(pos) > MOVED {
        // An early move: pan for the rest of the press; the marquee never grows.
        ctx.data_mut(|d| d.insert_temp(press_id(), Some(Press { panning: true, ..press })));
        return TouchVerdict::Panning { delta };
    }
    TouchVerdict::Pressing
}

fn press_start_held(start: Option<f64>, time: f64) -> bool {
    start.is_some_and(|t| time - t > HOLD)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::TouchVerdict;

    struct Bench {
        verdicts: Vec<TouchVerdict>,
        at: egui::Pos2,
    }

    fn frame(b: &mut Bench, ctx: &egui::Context, events: Vec<egui::Event>, at: f64, engine_owned: bool) {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(300.0, 200.0));
        let warm = events.is_empty();
        let input = egui::RawInput { screen_rect: Some(screen), time: Some(at), events, ..Default::default() };
        let mut run = ctx.run_ui(input, |ui| {
            let resp = ui.allocate_rect(ui.available_rect_before_wrap(), egui::Sense::drag());
            let v = super::verdict(ui, &resp, engine_owned);
            b.verdicts.push(v);
            if warm {
                b.at = resp.rect.center();
            }
        });
        run.textures_delta.clear();
    }

    fn bench() -> (egui::Context, Bench) {
        (egui::Context::default(), Bench { verdicts: Vec::new(), at: egui::Pos2::ZERO })
    }

    fn touch(phase: egui::TouchPhase, pos: egui::Pos2) -> egui::Event {
        egui::Event::Touch { device_id: egui::TouchDeviceId(1), id: egui::TouchId(1), phase, pos, force: Some(0.5) }
    }

    fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() }
    }

    /// A finger that moves early pans and stays panning even after the hold would have expired:
    /// the marquee never grows.
    #[test]
    fn an_early_finger_move_pans_and_stays_panning() {
        let (ctx, mut b) = bench();
        frame(&mut b, &ctx, vec![], -0.1, false);
        let at = b.at;
        frame(&mut b, &ctx, vec![touch(egui::TouchPhase::Start, at), egui::Event::PointerMoved(at), button(at, true)], 0.0, false);
        assert_eq!(b.verdicts.last(), Some(&TouchVerdict::Pressing), "a wait at first: {:?}", b.verdicts);
        let p = egui::pos2(at.x, at.y - 10.0);
        frame(&mut b, &ctx, vec![touch(egui::TouchPhase::Move, p), egui::Event::PointerMoved(p)], 0.2, false);
        assert!(matches!(b.verdicts.last(), Some(TouchVerdict::Panning { delta }) if delta.y < 0.0), "an early move pans: {:?}", b.verdicts);
        // Past the hold expiry the press stays a pan, not a selection-growth.
        let p2 = egui::pos2(at.x, at.y - 20.0);
        frame(&mut b, &ctx, vec![touch(egui::TouchPhase::Move, p2), egui::Event::PointerMoved(p2)], 0.9, false);
        assert!(matches!(b.verdicts.last(), Some(TouchVerdict::Panning { .. })), "the latch holds: {:?}", b.verdicts);
        frame(&mut b, &ctx, vec![button(p2, false)], 1.0, false);
        assert_eq!(b.verdicts.last(), Some(&TouchVerdict::None), "the lift ends the arbitration: {:?}", b.verdicts);
    }

    /// A finger held still past the expiry is left to grow a selection (the desktop drag's
    /// behaviour) and to egui's long-press menu; moving after the hold never pans.
    #[test]
    fn a_still_hold_grows_selection_and_only_then_moves() {
        let (ctx, mut b) = bench();
        frame(&mut b, &ctx, vec![], -0.1, false);
        let at = b.at;
        frame(&mut b, &ctx, vec![touch(egui::TouchPhase::Start, at), egui::Event::PointerMoved(at), button(at, true)], 0.0, false);
        for k in 1..=7 {
            frame(&mut b, &ctx, vec![touch(egui::TouchPhase::Move, at), egui::Event::PointerMoved(at)], 0.1 * k as f64, false);
        }
        assert_eq!(b.verdicts.last(), Some(&TouchVerdict::Owned), "a still hold is the engine's: {:?}", b.verdicts);
        assert!(!b.verdicts.iter().any(|v| matches!(v, TouchVerdict::Panning { .. })), "never pans: {:?}", b.verdicts);
        let p = egui::pos2(at.x, at.y - 12.0);
        frame(&mut b, &ctx, vec![touch(egui::TouchPhase::Move, p), egui::Event::PointerMoved(p)], 0.8, false);
        assert_eq!(b.verdicts.last(), Some(&TouchVerdict::Owned), "a move after the hold grows, not pans: {:?}", b.verdicts);
    }

    /// A press the engine owns (shape, caret, handle) is never arbitrated: it stays Owned from
    /// the first frame, and an early move never pans.
    #[test]
    fn an_engine_owned_press_is_never_arbitrated() {
        let (ctx, mut b) = bench();
        frame(&mut b, &ctx, vec![], -0.1, false);
        let at = b.at;
        frame(&mut b, &ctx, vec![touch(egui::TouchPhase::Start, at), egui::Event::PointerMoved(at), button(at, true)], 0.0, true);
        assert_eq!(b.verdicts.last(), Some(&TouchVerdict::Owned), "owned at once: {:?}", b.verdicts);
        let p = egui::pos2(at.x, at.y - 10.0);
        frame(&mut b, &ctx, vec![touch(egui::TouchPhase::Move, p), egui::Event::PointerMoved(p)], 0.2, true);
        assert!(!b.verdicts.iter().any(|v| matches!(v, TouchVerdict::Panning { .. })), "the drag moves the shape, not the canvas: {:?}", b.verdicts);
    }

    /// A mouse press is none of the arbiter's business, however far it travels.
    #[test]
    fn a_mouse_press_is_never_arbitrated() {
        let (ctx, mut b) = bench();
        frame(&mut b, &ctx, vec![], -0.1, false);
        let at = b.at;
        frame(&mut b, &ctx, vec![egui::Event::PointerMoved(at), button(at, true)], 0.0, false);
        for k in 1..=5 {
            let p = egui::pos2(at.x, at.y - 10.0 * k as f32);
            frame(&mut b, &ctx, vec![egui::Event::PointerMoved(p)], 0.1 * k as f64, false);
        }
        assert!(b.verdicts.iter().skip(1).all(|v| *v == TouchVerdict::None), "a mouse press is ignored: {:?}", b.verdicts);
    }
}
