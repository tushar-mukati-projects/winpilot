use std::time::{Duration, Instant};

const NANOS_PER_SECOND: u64 = 1_000_000_000;
const HUNDRED_NANOS_PER_SECOND: u64 = 10_000_000;
const CENTISECONDS_PER_SECOND: u64 = 100;

#[derive(Debug, Clone, Copy)]
pub(crate) struct TickClock {
    start: Instant,
    fps: u32,
}

impl TickClock {
    pub(crate) fn new(start: Instant, fps: u32) -> Self {
        Self { start, fps: fps.max(1) }
    }

    pub(crate) fn wait_for(&self, tick: u64, now: Instant) -> Duration {
        (self.start + tick_offset(tick, self.fps)).saturating_duration_since(now)
    }

    pub(crate) fn due(&self, now: Instant) -> u64 {
        ticks_elapsed(now.saturating_duration_since(self.start), self.fps)
    }
}

pub(crate) fn tick_offset(tick: u64, fps: u32) -> Duration {
    let nanos = (u128::from(tick) * u128::from(NANOS_PER_SECOND)).div_ceil(u128::from(fps.max(1)));
    Duration::from_nanos(u64::try_from(nanos).unwrap_or(u64::MAX))
}

pub(crate) fn ticks_elapsed(elapsed: Duration, fps: u32) -> u64 {
    let ticks = elapsed.as_nanos() * u128::from(fps.max(1)) / u128::from(NANOS_PER_SECOND);
    u64::try_from(ticks).unwrap_or(u64::MAX)
}

pub(crate) fn total_ticks(fps: u32, seconds: u32) -> u64 {
    u64::from(fps.max(1)) * u64::from(seconds)
}

pub(crate) fn sample_time(tick: u64, fps: u32) -> i64 {
    let units = u128::from(tick) * u128::from(HUNDRED_NANOS_PER_SECOND) / u128::from(fps.max(1));
    i64::try_from(units).unwrap_or(i64::MAX)
}

pub(crate) fn gif_delay(first_tick: u64, end_tick: u64, fps: u32) -> u64 {
    let centiseconds =
        |tick: u64| (u128::from(tick) * u128::from(CENTISECONDS_PER_SECOND)).div_ceil(u128::from(fps.max(1)));
    let delay = centiseconds(end_tick).saturating_sub(centiseconds(first_tick));
    u64::try_from(delay).unwrap_or(u64::MAX)
}

pub(crate) fn seconds_of(ticks: u64, fps: u32) -> f64 {
    ticks as f64 / f64::from(fps.max(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_times_follow_the_tick_index_exactly() {
        assert_eq!(sample_time(0, 30), 0);
        assert_eq!(sample_time(1, 30), 333_333);
        assert_eq!(sample_time(30, 30), 10_000_000);
        assert_eq!(sample_time(3, 60), 500_000);
        assert_eq!(sample_time(108_000, 60), 18_000_000_000);
    }

    #[test]
    fn consecutive_sample_spans_add_up_to_whole_seconds() {
        for fps in [1, 7, 24, 30, 60] {
            let span = |tick: u64| sample_time(tick + 1, fps) - sample_time(tick, fps);
            let total: i64 = (0..u64::from(fps)).map(span).sum();
            assert_eq!(total, 10_000_000, "{fps} fps");
        }
        assert_eq!(sample_time(1, 30) - sample_time(0, 30), 333_333);
        assert_eq!(sample_time(3, 30) - sample_time(2, 30), 333_334);
    }

    #[test]
    fn tick_offsets_and_due_ticks_agree() {
        assert_eq!(tick_offset(0, 30), Duration::ZERO);
        assert_eq!(tick_offset(30, 30), Duration::from_secs(1));
        assert_eq!(tick_offset(1, 3), Duration::from_nanos(333_333_334));
        for tick in [0u64, 1, 29, 30, 31, 899] {
            assert_eq!(ticks_elapsed(tick_offset(tick, 30), 30), tick, "tick {tick}");
        }
        assert_eq!(ticks_elapsed(Duration::from_millis(999), 30), 29);
        assert_eq!(ticks_elapsed(Duration::from_millis(1000), 30), 30);
    }

    #[test]
    fn the_clock_waits_until_each_tick_and_reports_late_ticks() {
        let start = Instant::now();
        let clock = TickClock::new(start, 10);
        assert_eq!(clock.wait_for(0, start), Duration::ZERO);
        assert_eq!(clock.wait_for(3, start), Duration::from_millis(300));
        assert_eq!(clock.wait_for(3, start + Duration::from_millis(500)), Duration::ZERO);
        assert_eq!(clock.due(start + Duration::from_millis(250)), 2);
        assert_eq!(TickClock::new(start, 0).due(start + Duration::from_secs(2)), 2);
    }

    #[test]
    fn totals_cover_the_whole_duration() {
        assert_eq!(total_ticks(30, 300), 9000);
        assert_eq!(total_ticks(60, 1800), 108_000);
        assert_eq!(total_ticks(0, 5), 5);
        assert!((seconds_of(372, 30) - 12.4).abs() < 1e-9);
    }

    #[test]
    fn gif_delays_track_the_timeline_without_drift() {
        assert_eq!(gif_delay(0, 1, 20), 5);
        assert_eq!(gif_delay(0, 1, 15), 7);
        assert_eq!(gif_delay(1, 2, 15), 7);
        assert_eq!(gif_delay(2, 3, 15), 6);
        let per_frame: u64 = (0..15).map(|tick| gif_delay(tick, tick + 1, 15)).sum();
        assert_eq!(per_frame, 100);
        assert_eq!(gif_delay(0, 40, 20), 200);
        assert_eq!(gif_delay(5, 5, 20), 0);
        assert_eq!(gif_delay(0, 1800 * 20, 20), 180_000);
        assert_eq!(gif_delay(0, u64::MAX, 1), u64::MAX);
    }
}
