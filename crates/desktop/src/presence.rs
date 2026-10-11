use std::time::{Duration, Instant};

use deckhand_core::Rect;

use crate::{
    easing::{breath, lap_fraction, progress},
    fade::Fade,
    glide::Glide,
    glow_tints::GlowLook,
    overlay_frame::{BorderFrame, PillFrame},
    overlay_palette::Tone,
};

pub(crate) const FRAME: Duration = Duration::from_millis(15);
pub(crate) const FADE_IN: Duration = Duration::from_millis(180);
pub(crate) const FADE_OUT: Duration = Duration::from_millis(250);
pub(crate) const GLIDE: Duration = Duration::from_millis(160);
pub(crate) const STOP_HOLD: Duration = Duration::from_millis(1600);
pub(crate) const BREATH: Duration = Duration::from_millis(1600);
pub(crate) const IGNITE: Duration = Duration::from_millis(450);
pub(crate) const ENTRANCE: Duration = Duration::from_millis(520);
const FLOW_LAP: Duration = Duration::from_millis(3500);
const SHIMMER_LAP: Duration = Duration::from_millis(2600);
const SPIN_LAP: Duration = Duration::from_millis(4000);
const STOP_PULSE: Duration = Duration::from_millis(520);
const STILL_ALPHA: f32 = 0.92;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Anchor {
    Window,
    PillOnly,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Presence {
    glide: Glide,
    scale: f32,
    tone: Tone,
    fade: Fade,
    born: Instant,
    stopped_at: Option<Instant>,
    animated: bool,
    anchor: Anchor,
}

impl Presence {
    pub(crate) fn working(rect: Rect, scale: f32, animated: bool, now: Instant) -> Self {
        Self {
            glide: Glide::rest(rect, now),
            scale,
            tone: Tone::Working,
            fade: Fade::appear(timed(FADE_IN, animated), now),
            born: now,
            stopped_at: None,
            animated,
            anchor: Anchor::Window,
        }
    }

    pub(crate) fn stopped_banner(area: Rect, scale: f32, animated: bool, now: Instant) -> Self {
        Self {
            tone: Tone::Stopped,
            stopped_at: Some(now),
            anchor: Anchor::PillOnly,
            ..Self::working(area, scale, animated, now)
        }
    }

    pub(crate) fn retarget(&mut self, rect: Rect, scale: f32, animated: bool, now: Instant) {
        if self.tone == Tone::Stopped {
            let fade = self.fade.toward(1.0, timed(FADE_IN, animated), now);
            *self = Self { fade, ..Self::working(self.glide.at(now), scale, animated, now) };
        }
        if self.fade.is_leaving() {
            self.fade = self.fade.toward(1.0, timed(FADE_IN, animated), now);
        }
        if rect != self.glide.target() {
            self.glide = self.glide.toward(rect, timed(GLIDE, animated), now);
        }
        self.scale = scale;
        self.animated = animated;
    }

    pub(crate) fn leave(&mut self, now: Instant) {
        if self.tone == Tone::Working && !self.fade.is_leaving() {
            self.fade = self.fade.toward(0.0, timed(FADE_OUT, self.animated), now);
        }
    }

    pub(crate) fn stop(&mut self, now: Instant) {
        self.tone = Tone::Stopped;
        self.stopped_at = Some(now);
        if self.fade.is_leaving() {
            self.fade = self.fade.toward(1.0, timed(FADE_IN, self.animated), now);
        }
    }

    pub(crate) fn resume(&mut self, now: Instant) {
        if self.tone == Tone::Stopped && !self.fade.is_leaving() {
            self.fade = self.fade.toward(0.0, timed(FADE_OUT, self.animated), now);
        }
    }

    pub(crate) fn advance(&mut self, now: Instant) {
        let held = self.stopped_at.is_some_and(|since| now.saturating_duration_since(since) >= STOP_HOLD);
        if held && !self.fade.is_leaving() {
            self.fade = self.fade.toward(0.0, timed(FADE_OUT, self.animated), now);
        }
    }

    pub(crate) fn is_acting(&self) -> bool {
        self.tone == Tone::Working && self.anchor == Anchor::Window && !self.fade.is_leaving()
    }

    pub(crate) fn is_gone(&self, now: Instant) -> bool {
        self.fade.has_vanished(now)
    }

    pub(crate) fn border(&self, now: Instant) -> Option<BorderFrame> {
        if self.anchor == Anchor::PillOnly {
            return None;
        }
        let alpha = self.fade.level(now) * if self.animated { 1.0 } else { STILL_ALPHA };
        Some(BorderFrame {
            rect: self.glide.at(now),
            tone: self.tone,
            scale: self.scale,
            alpha,
            glow: self.glow(now),
            steady: !self.is_moving(now),
        })
    }

    pub(crate) fn pill(&self, now: Instant) -> PillFrame {
        let age = self.age(now);
        let lively = self.animated && self.tone == Tone::Working;
        PillFrame {
            anchor: self.glide.at(now),
            label_anchor: self.glide.target(),
            tone: self.tone,
            scale: self.scale,
            alpha: self.fade.level(now),
            pulse: if lively { breath(age, BREATH) } else { 1.0 },
            spin: if self.animated { lap_fraction(age, SPIN_LAP) } else { 0.0 },
            entrance: if self.animated { progress(age, ENTRANCE) } else { 1.0 },
            steady: !self.is_moving(now),
        }
    }

    pub(crate) fn next_wake(&self, now: Instant) -> Option<Duration> {
        if self.animated && (self.is_moving(now) || self.tone == Tone::Working) {
            return Some(FRAME);
        }
        match self.stopped_at {
            Some(since) if !self.fade.is_leaving() => {
                Some(STOP_HOLD.saturating_sub(now.saturating_duration_since(since)))
            }
            _ => None,
        }
    }

    fn glow(&self, now: Instant) -> GlowLook {
        if !self.animated {
            return GlowLook::STILL;
        }
        let age = self.age(now);
        GlowLook {
            flow: lap_fraction(age, FLOW_LAP),
            shimmer: lap_fraction(age, SHIMMER_LAP),
            ignite: (age < IGNITE).then(|| progress(age, IGNITE)),
            gain: 1.0 + 0.7 * self.stop_pulse(now),
        }
    }

    fn stop_pulse(&self, now: Instant) -> f32 {
        let Some(since) = self.stopped_at else { return 0.0 };
        let done = progress(now.saturating_duration_since(since), STOP_PULSE);
        (done * std::f32::consts::PI).sin().max(0.0)
    }

    fn is_moving(&self, now: Instant) -> bool {
        let pulsing = self.stopped_at.is_some_and(|since| now.saturating_duration_since(since) < STOP_PULSE);
        let entering = self.age(now) < ENTRANCE.max(IGNITE);
        !self.fade.is_settled(now) || !self.glide.is_settled(now) || pulsing || entering
    }

    fn age(&self, now: Instant) -> Duration {
        now.saturating_duration_since(self.born)
    }
}

pub(crate) fn timed(duration: Duration, animated: bool) -> Duration {
    if animated { duration } else { Duration::ZERO }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;
    const RECT: Rect = Rect::new(100, 100, 800, 600);

    #[test]
    fn the_border_ignites_then_flows_steadily() {
        let start = Instant::now();
        let presence = Presence::working(RECT, 1.0, true, start);
        let first = presence.border(start).unwrap();
        assert_eq!((first.alpha, first.glow.ignite, first.steady), (0.0, Some(0.0), false));
        let settled = presence.border(start + MS(1000)).unwrap();
        assert_eq!((settled.alpha, settled.glow.ignite, settled.steady), (1.0, None, true));
        assert!((settled.glow.flow - 1000.0 / 3500.0).abs() < 1e-4);
        assert!(!crate::easing::close(presence.border(start + MS(2000)).unwrap().glow.flow, settled.glow.flow));
    }

    #[test]
    fn retargeting_glides_to_the_new_rect() {
        let start = Instant::now();
        let mut presence = Presence::working(RECT, 1.0, true, start);
        let target = Rect::new(500, 300, 400, 300);
        presence.retarget(target, 1.0, true, start + MS(500));
        assert_eq!(presence.border(start + MS(500)).unwrap().rect, RECT);
        let midway = presence.border(start + MS(560)).unwrap().rect;
        assert!(midway != RECT && midway != target);
        assert_eq!(presence.border(start + MS(660)).unwrap().rect, target);
        assert_eq!(presence.pill(start + MS(560)).label_anchor, target);
    }

    #[test]
    fn leaving_fades_out_over_a_quarter_second() {
        let start = Instant::now();
        let mut presence = Presence::working(RECT, 1.0, true, start);
        presence.leave(start + MS(1000));
        assert!(!presence.is_gone(start + MS(1200)));
        assert!(presence.is_gone(start + MS(1250)));
    }

    #[test]
    fn coming_back_while_leaving_fades_in_again() {
        let start = Instant::now();
        let mut presence = Presence::working(RECT, 1.0, true, start);
        presence.leave(start + MS(1000));
        presence.retarget(RECT, 1.0, true, start + MS(1100));
        assert!(!presence.is_gone(start + MS(2000)));
        assert!(presence.pill(start + MS(1400)).alpha > 0.99);
    }

    #[test]
    fn stopping_turns_red_holds_then_fades_and_ignores_leave() {
        let start = Instant::now();
        let mut presence = Presence::working(RECT, 1.0, true, start);
        presence.stop(start + MS(500));
        presence.leave(start + MS(600));
        let held = start + MS(1500);
        let border = presence.border(held).unwrap();
        assert_eq!((border.tone, border.alpha, border.steady), (Tone::Stopped, 1.0, true));
        assert!(presence.border(start + MS(760)).unwrap().glow.gain > 1.6);
        assert_eq!(presence.next_wake(held), Some(MS(600)));
        presence.advance(start + MS(2100));
        assert!(!presence.is_gone(start + MS(2300)));
        assert!(presence.is_gone(start + MS(2350)));
    }

    #[test]
    fn resuming_drops_the_stopped_banner() {
        let start = Instant::now();
        let mut presence = Presence::stopped_banner(RECT, 1.0, true, start);
        assert!(presence.border(start).is_none());
        presence.resume(start + MS(300));
        assert!(presence.is_gone(start + MS(550)));
    }

    #[test]
    fn reduced_motion_is_static_and_needs_no_frames() {
        let start = Instant::now();
        let mut presence = Presence::working(RECT, 1.0, false, start);
        let border = presence.border(start).unwrap();
        assert_eq!((border.alpha, border.glow), (STILL_ALPHA, GlowLook::STILL));
        assert_eq!((presence.pill(start).pulse, presence.pill(start).entrance), (1.0, 1.0));
        assert_eq!(presence.next_wake(start), None);
        presence.retarget(Rect::new(0, 0, 50, 50), 1.0, false, start);
        assert_eq!(presence.border(start).unwrap().rect, Rect::new(0, 0, 50, 50));
        presence.leave(start);
        assert!(presence.is_gone(start));
    }
}
