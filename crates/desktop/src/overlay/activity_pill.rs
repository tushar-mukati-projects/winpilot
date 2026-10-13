use std::time::{Duration, Instant};

use deckhand_core::Rect;

use super::{
    easing::{breath, lap_fraction, progress},
    fade::Fade,
    presence::{BREATH, FRAME, timed},
    scene_frame::ActivityFrame,
    scene_items::Look,
};

pub(crate) const ACTIVITY_HOLD: Duration = Duration::from_secs(2);
const FADE_IN: Duration = Duration::from_millis(180);
const FADE_OUT: Duration = Duration::from_millis(300);
const ENTRANCE: Duration = Duration::from_millis(520);
const SPIN_LAP: Duration = Duration::from_millis(4000);

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ActivityPill {
    area: Rect,
    hint: String,
    look: Look,
    fade: Fade,
    born: Instant,
    last: Instant,
}

impl ActivityPill {
    pub(crate) fn new(area: Rect, hint: String, look: Look, now: Instant) -> Self {
        Self { area, hint, look, fade: Fade::appear(timed(FADE_IN, look.animated), now), born: now, last: now }
    }

    pub(crate) fn update(&mut self, area: Rect, hint: String, look: Look, now: Instant) {
        if self.fade.is_leaving() {
            self.fade = self.fade.toward(1.0, timed(FADE_IN, look.animated), now);
        }
        self.area = area;
        self.hint = hint;
        self.look = look;
        self.last = now;
    }

    pub(crate) fn dismiss(&mut self, now: Instant) {
        if !self.fade.is_leaving() {
            self.fade = self.fade.toward(0.0, timed(FADE_OUT, self.look.animated), now);
        }
    }

    pub(crate) fn advance(&mut self, now: Instant) {
        if self.idle_for(now) >= ACTIVITY_HOLD {
            self.dismiss(now);
        }
    }

    pub(crate) fn is_gone(&self, now: Instant) -> bool {
        self.fade.has_vanished(now)
    }

    pub(crate) fn frame(&self, now: Instant) -> ActivityFrame<'_> {
        let age = now.saturating_duration_since(self.born);
        let animated = self.look.animated;
        ActivityFrame {
            anchor: self.area,
            hint: &self.hint,
            scale: self.look.scale,
            alpha: self.fade.level(now),
            pulse: if animated { breath(age, BREATH) } else { 1.0 },
            spin: if animated { lap_fraction(age, SPIN_LAP) } else { 0.0 },
            entrance: if animated { progress(age, ENTRANCE) } else { 1.0 },
            steady: self.fade.is_settled(now) && age >= ENTRANCE,
        }
    }

    pub(crate) fn wake(&self, now: Instant) -> Duration {
        if self.look.animated {
            return FRAME;
        }
        ACTIVITY_HOLD.saturating_sub(self.idle_for(now))
    }

    fn idle_for(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.last)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;
    const AREA: Rect = Rect::new(0, 0, 1920, 1032);
    const ANIMATED: Look = Look { scale: 1.0, animated: true };
    const STILL: Look = Look { scale: 1.0, animated: false };

    fn shown(look: Look, start: Instant) -> ActivityPill {
        ActivityPill::new(AREA, " \u{00B7} working in CapCut".to_string(), look, start)
    }

    #[test]
    fn it_fades_in_holds_for_two_seconds_then_fades_out() {
        let start = Instant::now();
        let mut pill = shown(ANIMATED, start);
        assert!(pill.frame(start).alpha < 0.01);
        assert!(pill.frame(start + MS(200)).alpha > 0.99);
        pill.advance(start + MS(1990));
        assert!(!pill.fade.is_leaving());
        pill.advance(start + MS(2000));
        assert!(pill.fade.is_leaving() && !pill.is_gone(start + MS(2200)));
        assert!(pill.is_gone(start + MS(2300)));
    }

    #[test]
    fn each_update_restarts_the_hold_and_changes_the_words() {
        let start = Instant::now();
        let mut pill = shown(ANIMATED, start);
        pill.update(AREA, " \u{00B7} click".to_string(), ANIMATED, start + MS(1500));
        pill.advance(start + MS(3000));
        assert!(!pill.fade.is_leaving());
        assert_eq!(pill.frame(start + MS(3000)).hint, " \u{00B7} click");
        pill.advance(start + MS(3500));
        assert!(pill.fade.is_leaving());
    }

    #[test]
    fn an_update_while_fading_brings_it_back() {
        let start = Instant::now();
        let mut pill = shown(ANIMATED, start);
        pill.advance(start + MS(2100));
        pill.update(AREA, " \u{00B7} typing".to_string(), ANIMATED, start + MS(2200));
        assert!(!pill.is_gone(start + MS(2600)));
        assert!(pill.frame(start + MS(2600)).alpha > 0.99);
    }

    #[test]
    fn dismissing_fades_it_at_once() {
        let start = Instant::now();
        let mut pill = shown(ANIMATED, start);
        pill.dismiss(start + MS(500));
        assert!(pill.is_gone(start + MS(800)));
    }

    #[test]
    fn reduced_motion_only_wakes_when_the_hold_ends() {
        let start = Instant::now();
        let mut pill = shown(STILL, start);
        assert_eq!(pill.wake(start + MS(500)), MS(1500));
        let frame = pill.frame(start);
        assert_eq!((frame.alpha, frame.pulse, frame.spin, frame.entrance), (1.0, 1.0, 0.0, 1.0));
        pill.advance(start + MS(2000));
        assert!(pill.is_gone(start + MS(2000)));
        assert_eq!(shown(ANIMATED, start).wake(start), FRAME);
    }
}
