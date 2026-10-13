use std::time::{Duration, Instant};

use winpilot_core::Rect;

pub const COVER_HOLD: Duration = Duration::from_millis(400);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverHold {
    hold: Duration,
    seen: Vec<(Rect, Instant)>,
    whole_frame_seen: Option<Instant>,
}

impl Default for CoverHold {
    fn default() -> Self {
        Self::new(COVER_HOLD)
    }
}

impl CoverHold {
    const fn new(hold: Duration) -> Self {
        Self { hold, seen: Vec::new(), whole_frame_seen: None }
    }

    pub fn rects(&mut self, current: &[Rect], now: Instant) -> Vec<Rect> {
        let hold = self.hold;
        self.seen.retain(|(rect, at)| !current.contains(rect) && now.saturating_duration_since(*at) < hold);
        for rect in current {
            if !self.seen.iter().any(|(seen, _)| seen == rect) {
                self.seen.push((*rect, now));
            }
        }
        self.seen.iter().map(|(rect, _)| *rect).collect()
    }

    pub fn whole_frame(&mut self, covered_now: bool, now: Instant) -> bool {
        if covered_now {
            self.whole_frame_seen = Some(now);
        }
        self.whole_frame_seen.is_some_and(|at| now.saturating_duration_since(at) < self.hold)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;
    const VAULT: Rect = Rect::new(100, 100, 400, 300);
    const MOVED: Rect = Rect::new(600, 100, 400, 300);

    #[test]
    fn a_moved_window_is_covered_at_both_places_for_the_hold() {
        let start = Instant::now();
        let mut hold = CoverHold::default();
        assert_eq!(hold.rects(&[VAULT], start), [VAULT]);
        assert_eq!(hold.rects(&[MOVED], start + MS(33)), [VAULT, MOVED]);
        assert_eq!(hold.rects(&[MOVED], start + MS(399)), [VAULT, MOVED]);
        assert_eq!(hold.rects(&[MOVED], start + MS(400)), [MOVED]);
    }

    #[test]
    fn a_window_that_disappears_or_minimizes_stays_covered_for_the_hold() {
        let start = Instant::now();
        let mut hold = CoverHold::default();
        hold.rects(&[VAULT], start);
        hold.rects(&[VAULT], start + MS(100));
        assert_eq!(hold.rects(&[], start + MS(133)), [VAULT]);
        assert_eq!(hold.rects(&[], start + MS(499)), [VAULT]);
        assert!(hold.rects(&[], start + MS(500)).is_empty());
    }

    #[test]
    fn a_window_that_stays_put_is_listed_once() {
        let start = Instant::now();
        let mut hold = CoverHold::default();
        for frame in 0..30 {
            assert_eq!(hold.rects(&[VAULT, VAULT], start + MS(frame * 33)), [VAULT]);
        }
    }

    #[test]
    fn the_whole_frame_stays_black_for_the_hold_after_a_deny_ends() {
        let start = Instant::now();
        let mut hold = CoverHold::default();
        assert!(!hold.whole_frame(false, start));
        assert!(hold.whole_frame(true, start + MS(10)));
        assert!(hold.whole_frame(false, start + MS(409)));
        assert!(!hold.whole_frame(false, start + MS(410)));
        assert!(hold.whole_frame(true, start + MS(500)));
    }

    #[test]
    fn no_hold_covers_only_the_present() {
        let start = Instant::now();
        let mut hold = CoverHold::new(Duration::ZERO);
        hold.rects(&[VAULT], start);
        assert_eq!(hold.rects(&[MOVED], start), [MOVED]);
        assert!(!hold.whole_frame(false, start));
    }
}
