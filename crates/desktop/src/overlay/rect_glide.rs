use std::time::{Duration, Instant};

use winpilot_core::Rect;

use super::easing::{ease_out_cubic, lerp, progress};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Glide {
    from: Rect,
    to: Rect,
    started: Instant,
    duration: Duration,
}

impl Glide {
    pub(crate) fn rest(rect: Rect, now: Instant) -> Self {
        Self { from: rect, to: rect, started: now, duration: Duration::ZERO }
    }

    pub(crate) fn at(&self, now: Instant) -> Rect {
        let eased = ease_out_cubic(progress(now.saturating_duration_since(self.started), self.duration));
        let edge = |from: i32, to: i32| lerp(from as f32, to as f32, eased).round() as i32;
        Rect::from_edges(
            edge(self.from.x, self.to.x),
            edge(self.from.y, self.to.y),
            edge(self.from.right(), self.to.right()),
            edge(self.from.bottom(), self.to.bottom()),
        )
    }

    pub(crate) fn toward(&self, to: Rect, duration: Duration, now: Instant) -> Self {
        Self { from: self.at(now), to, started: now, duration }
    }

    pub(crate) const fn target(&self) -> Rect {
        self.to
    }

    pub(crate) fn is_settled(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.started) >= self.duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    #[test]
    fn a_glide_eases_from_the_old_rect_to_the_new_one() {
        let start = Instant::now();
        let from = Rect::new(0, 0, 100, 100);
        let to = Rect::new(400, 200, 300, 100);
        let glide = Glide::rest(from, start).toward(to, MS(160), start);
        assert_eq!(glide.at(start), from);
        let halfway = glide.at(start + MS(80));
        assert!(halfway.x > 300 && halfway.x < 400);
        assert_eq!(glide.at(start + MS(160)), to);
        assert!(glide.is_settled(start + MS(160)) && !glide.is_settled(start + MS(100)));
        assert_eq!(glide.target(), to);
    }

    #[test]
    fn retargeting_mid_glide_starts_from_where_it_is() {
        let start = Instant::now();
        let glide = Glide::rest(Rect::new(0, 0, 10, 10), start).toward(Rect::new(100, 0, 10, 10), MS(160), start);
        let midway = glide.at(start + MS(40));
        let turned = glide.toward(Rect::new(0, 300, 10, 10), MS(160), start + MS(40));
        assert_eq!(turned.at(start + MS(40)), midway);
    }
}
