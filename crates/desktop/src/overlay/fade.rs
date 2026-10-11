use std::time::{Duration, Instant};

use super::easing::{ease_out_cubic, lerp, progress};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Fade {
    from: f32,
    to: f32,
    started: Instant,
    duration: Duration,
}

impl Fade {
    pub(crate) fn appear(duration: Duration, now: Instant) -> Self {
        Self { from: 0.0, to: 1.0, started: now, duration }
    }

    pub(crate) fn level(&self, now: Instant) -> f32 {
        let done = progress(now.saturating_duration_since(self.started), self.duration);
        lerp(self.from, self.to, ease_out_cubic(done))
    }

    pub(crate) fn toward(&self, to: f32, duration: Duration, now: Instant) -> Self {
        Self { from: self.level(now), to, started: now, duration }
    }

    pub(crate) fn is_settled(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.started) >= self.duration
    }

    pub(crate) fn is_leaving(&self) -> bool {
        self.to <= 0.0
    }

    pub(crate) fn has_vanished(&self, now: Instant) -> bool {
        self.is_leaving() && self.is_settled(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn appearing_rises_from_zero_to_one() {
        let start = Instant::now();
        let fade = Fade::appear(MS(180), start);
        assert!(crate::overlay::easing::close(fade.level(start), 0.0));
        assert!(fade.level(start + MS(90)) > 0.5);
        assert!(crate::overlay::easing::close(fade.level(start + MS(180)), 1.0));
        assert!(fade.is_settled(start + MS(180)) && !fade.is_leaving());
    }

    #[test]
    fn leaving_starts_from_the_current_level() {
        let start = Instant::now();
        let fade = Fade::appear(MS(180), start);
        let leaving = fade.toward(0.0, MS(250), start + MS(45));
        let level = fade.level(start + MS(45));
        assert!(crate::overlay::easing::close(leaving.level(start + MS(45)), level));
        assert!(leaving.is_leaving() && !leaving.has_vanished(start + MS(200)));
        assert!(leaving.has_vanished(start + MS(295)));
    }

    #[test]
    fn instant_fades_settle_immediately() {
        let start = Instant::now();
        let fade = Fade::appear(Duration::ZERO, start);
        assert!(crate::overlay::easing::close(fade.level(start), 1.0));
        assert!(fade.toward(0.0, Duration::ZERO, start).has_vanished(start));
    }
}
