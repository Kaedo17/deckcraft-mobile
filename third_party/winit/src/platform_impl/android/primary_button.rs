// The primary mouse button's dedupe. Android reports the primary button twice per
// transition — as `Down`/`Up` (the finger-like pair) and as `ButtonPress`/`ButtonRelease`
// (the button pair) — so one physical click would reach the app as two, and a
// double-click as four. `Reported` collapses each transition to one report.
//
// This file is included verbatim by the deckcraft workspace's tests
// (`crates/ui-egui/tests/android_primary_button.rs`), so it uses only `std` and
// plain comments where a module's `//!` would not survive the include.

use std::cell::Cell;

/// What the primary button's state was last reported as.
#[derive(Debug, Default)]
pub(crate) struct Reported(Cell<Side>);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum Side {
    #[default]
    Released,
    Pressed,
}

impl Reported {
    /// Report a transition of the primary button (`true`: pressed, `false`: released).
    /// Returns `true` when this is a new transition — dispatch it — and `false` when the
    /// same transition was already reported for this gesture (Android's duplicate: swallow).
    pub(crate) fn report(&self, pressed: bool) -> bool {
        let want = if pressed { Side::Pressed } else { Side::Released };
        if self.0.get() == want {
            return false;
        }
        self.0.set(want);
        true
    }

    /// The gesture ended without its final transition (`Cancel`): forget the state, so
    /// the next gesture's press reports again.
    pub(crate) fn reset(&self) {
        self.0.set(Side::Released);
    }
}

#[cfg(test)]
mod tests {
    use super::Reported;

    /// Android's report of one physical click (`Down` + `ButtonPress`, then the mirror
    /// on release): exactly one press and one release reach the app.
    #[test]
    fn one_physical_click_reports_one_press_and_one_release() {
        let r = Reported::default();
        assert!(r.report(true), "the Down starts the press");
        assert!(!r.report(true), "the ButtonPress is the same press");
        assert!(r.report(false), "the ButtonRelease ends it");
        assert!(!r.report(false), "the Up is the same release");
    }

    /// Two rapid genuine clicks — every transition seen exactly once — all report.
    #[test]
    fn two_genuine_clicks_report_every_transition() {
        let r = Reported::default();
        assert!(r.report(true), "first press");
        assert!(r.report(false), "first release");
        assert!(r.report(true), "second press");
        assert!(r.report(false), "second release");
    }

    /// A device that only sends `ButtonPress`/`ButtonRelease` (no bare `Down`/`Up`).
    #[test]
    fn a_button_press_only_device_reports_both_transitions() {
        let r = Reported::default();
        assert!(r.report(true) && r.report(false));
    }

    /// `Cancel` ends the gesture without its final transition: the next gesture's
    /// press must report again (a stuck state would silently drop a real click).
    #[test]
    fn a_cancelled_gesture_reports_the_next_press() {
        let r = Reported::default();
        assert!(r.report(true));
        r.reset();
        assert!(r.report(true), "the next gesture's press is a new transition");
    }
}
