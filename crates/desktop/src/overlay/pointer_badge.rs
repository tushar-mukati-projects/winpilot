use std::time::{Duration, Instant};

use deckhand_core::{Point, glide_timing::glide_point};

use super::{easing::progress, fade::Fade, presence::FRAME, scene_frame::BadgeFrame, scene_items::Look};

const BADGE_LINGER: Duration = Duration::from_millis(1200);
const BADGE_FADE_IN: Duration = Duration::from_millis(160);
const BADGE_FADE_OUT: Duration = Duration::from_millis(300);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PointerBadge {
    from: Point,
    to: Point,
    started: Instant,
    duration: Duration,
    look: Look,
    fade: Fade,
}

impl PointerBadge {
    pub(crate) fn new(from: Point, to: Point, duration: Duration, look: Look, now: Instant) -> Self {
        let fade = Fade::appear(if look.animated { BADGE_FADE_IN } else { Duration::ZERO }, now);
        Self { from, to, started: now, duration, look, fade }
    }

    pub(crate) fn go(&mut self, from: Point, to: Point, duration: Duration, look: Look, now: Instant) {
        let fade = if self.fade.is_leaving() { self.fade.toward(1.0, BADGE_FADE_IN, now) } else { self.fade };
        *self = Self { from, to, started: now, duration, look, fade };
    }

    pub(crate) fn leave(&mut self, now: Instant) {
        if !self.fade.is_leaving() {
            let duration = if self.look.animated { BADGE_FADE_OUT } else { Duration::ZERO };
            self.fade = self.fade.toward(0.0, duration, now);
        }
    }

    pub(crate) fn advance(&mut self, now: Instant) {
        if now >= self.idle_from() + BADGE_LINGER {
            self.leave(now);
        }
    }

    pub(crate) fn is_gone(&self, now: Instant) -> bool {
        self.fade.has_vanished(now)
    }

    pub(crate) fn at(&self, now: Instant) -> Point {
        let done = progress(now.saturating_duration_since(self.started), self.duration);
        glide_point(self.from, self.to, f64::from(done))
    }

    pub(crate) fn frame(&self, now: Instant) -> BadgeFrame {
        BadgeFrame { at: self.at(now), alpha: self.fade.level(now), scale: self.look.scale }
    }

    pub(crate) fn wake(&self, now: Instant) -> Duration {
        let moving = now < self.idle_from() || !self.fade.is_settled(now);
        if moving && self.look.animated {
            return FRAME;
        }
        if self.fade.is_leaving() {
            return FRAME;
        }
        (self.idle_from() + BADGE_LINGER).saturating_duration_since(now).max(FRAME)
    }

    fn idle_from(&self) -> Instant {
        self.started + self.duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;
    const LOOK: Look = Look { scale: 1.0, animated: true };

    #[test]
    fn the_badge_follows_the_glide_path() {
        let start = Instant::now();
        let badge = PointerBadge::new(Point::new(0, 0), Point::new(400, 200), MS(200), LOOK, start);
        assert_eq!(badge.at(start), Point::new(0, 0));
        assert_eq!(badge.at(start + MS(100)), Point::new(200, 100));
        assert_eq!(badge.at(start + MS(200)), Point::new(400, 200));
        assert_eq!(badge.wake(start + MS(50)), FRAME);
    }

    #[test]
    fn a_new_move_starts_from_the_real_cursor_and_jumps_when_instant() {
        let start = Instant::now();
        let mut badge = PointerBadge::new(Point::new(0, 0), Point::new(400, 0), MS(200), LOOK, start);
        badge.leave(start + MS(300));
        badge.go(Point::new(50, 50), Point::new(0, 300), MS(200), LOOK, start + MS(310));
        assert_eq!(badge.at(start + MS(310)), Point::new(50, 50));
        assert!(!badge.is_gone(start + MS(700)));
        badge.go(Point::new(5, 5), Point::new(10, 10), Duration::ZERO, LOOK, start + MS(800));
        assert_eq!(badge.at(start + MS(800)), Point::new(10, 10));
    }

    #[test]
    fn the_badge_lingers_then_fades() {
        let start = Instant::now();
        let mut badge = PointerBadge::new(Point::new(0, 0), Point::new(10, 0), Duration::ZERO, LOOK, start);
        assert_eq!(badge.wake(start + MS(200)), MS(1000));
        badge.advance(start + MS(1100));
        assert!(!badge.is_gone(start + MS(1100)));
        badge.advance(start + MS(1200));
        assert!(badge.is_gone(start + MS(1500)));
    }
}
