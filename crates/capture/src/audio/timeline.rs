use crate::time_units::HNS_PER_SECOND;

const TOLERANCE_HNS: i64 = 300_000;
const IDLE_MARGIN_HNS: i64 = 2_500_000;
const MAX_IDLE_FILL_HNS: i64 = 10_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Placement {
    pub(crate) silence: u64,
    pub(crate) skip: u64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Timeline {
    origin_hns: i64,
    rate: u32,
    emitted: u64,
}

impl Timeline {
    pub(crate) const fn new(origin_hns: i64, rate: u32) -> Self {
        Self { origin_hns, rate, emitted: 0 }
    }

    pub(crate) fn place(&mut self, at_hns: Option<i64>, frames: u64) -> Placement {
        let placement = at_hns.map_or_else(Placement::default, |at| self.placement_at(at, frames));
        self.emitted += placement.silence + (frames - placement.skip);
        placement
    }

    pub(crate) fn idle_fill(&mut self, now_hns: i64) -> u64 {
        let behind = self.frames_until(now_hns - IDLE_MARGIN_HNS);
        if behind <= i128::from(self.frames_of(TOLERANCE_HNS)) {
            return 0;
        }
        let fill = (behind as u64).min(self.frames_of(MAX_IDLE_FILL_HNS));
        self.emitted += fill;
        fill
    }

    pub(crate) fn flush(&mut self, now_hns: i64) -> u64 {
        let fill = u64::try_from(self.frames_until(now_hns)).unwrap_or(0);
        self.emitted += fill;
        fill
    }

    fn placement_at(&self, at_hns: i64, frames: u64) -> Placement {
        let ahead = self.frames_until(at_hns);
        let tolerance = i128::from(self.frames_of(TOLERANCE_HNS));
        if ahead > tolerance {
            return Placement { silence: ahead as u64, skip: 0 };
        }
        if ahead < -tolerance {
            return Placement { silence: 0, skip: (ahead.unsigned_abs() as u64).min(frames) };
        }
        Placement::default()
    }

    fn frames_until(&self, at_hns: i64) -> i128 {
        let since_origin = i128::from(at_hns) - i128::from(self.origin_hns);
        since_origin * i128::from(self.rate) / i128::from(HNS_PER_SECOND) - i128::from(self.emitted)
    }

    fn frames_of(&self, hns: i64) -> u64 {
        (i128::from(hns) * i128::from(self.rate) / i128::from(HNS_PER_SECOND)) as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;
    const ORIGIN: i64 = 1_000_000_000;
    const MS: i64 = 10_000;

    fn timeline() -> Timeline {
        Timeline::new(ORIGIN, RATE)
    }

    #[test]
    fn contiguous_packets_are_placed_back_to_back() {
        let mut line = timeline();
        assert_eq!(line.place(Some(ORIGIN), 480), Placement::default());
        assert_eq!(line.place(Some(ORIGIN + 10 * MS), 480), Placement::default());
        assert_eq!(line.place(None, 480), Placement::default());
        assert_eq!(line.emitted, 1440);
    }

    #[test]
    fn small_timestamp_jitter_is_ignored() {
        let mut line = timeline();
        line.place(Some(ORIGIN), 480);
        assert_eq!(line.place(Some(ORIGIN + 25 * MS), 480), Placement::default());
        assert_eq!(line.place(Some(ORIGIN + 5 * MS), 480), Placement::default());
    }

    #[test]
    fn a_gap_between_packets_becomes_silence() {
        let mut line = timeline();
        line.place(Some(ORIGIN), 480);
        let placement = line.place(Some(ORIGIN + 2_000 * MS), 480);
        assert_eq!(placement, Placement { silence: 96_000 - 480, skip: 0 });
        assert_eq!(line.emitted, 96_480);
    }

    #[test]
    fn audio_from_before_the_origin_is_trimmed() {
        let mut line = timeline();
        let placement = line.place(Some(ORIGIN - 50 * MS), 4800);
        assert_eq!(placement, Placement { silence: 0, skip: 2400 });
        assert_eq!(line.emitted, 2400);
        let whole = timeline().place(Some(ORIGIN - 500 * MS), 4800);
        assert_eq!(whole.skip, 4800);
    }

    #[test]
    fn an_idle_stream_fills_silence_up_to_the_margin_and_no_further() {
        let mut line = timeline();
        assert_eq!(line.idle_fill(ORIGIN + 100 * MS), 0);
        assert_eq!(line.idle_fill(ORIGIN + 1_250 * MS), 48_000);
        assert_eq!(line.idle_fill(ORIGIN + 1_260 * MS), 0);
        assert_eq!(line.idle_fill(ORIGIN + 1_300 * MS), 2400);
        assert_eq!(line.place(Some(ORIGIN + 1_060 * MS), 480), Placement::default());
    }

    #[test]
    fn a_long_idle_stretch_is_filled_a_second_at_a_time() {
        let mut line = timeline();
        assert_eq!(line.idle_fill(ORIGIN + 60_000 * MS), 48_000);
        assert_eq!(line.idle_fill(ORIGIN + 60_000 * MS), 48_000);
    }

    #[test]
    fn a_packet_after_a_premature_fill_drops_its_overlap() {
        let mut line = timeline();
        line.idle_fill(ORIGIN + 1_250 * MS);
        let late = line.place(Some(ORIGIN + 900 * MS), 4800);
        assert_eq!(late, Placement { silence: 0, skip: 4800 });
    }

    #[test]
    fn flushing_fills_exactly_up_to_now() {
        let mut line = timeline();
        line.place(Some(ORIGIN), 480);
        assert_eq!(line.flush(ORIGIN + 1_000 * MS), 48_000 - 480);
        assert_eq!(line.flush(ORIGIN + 500 * MS), 0);
        assert_eq!(line.emitted, 48_000);
    }
}
