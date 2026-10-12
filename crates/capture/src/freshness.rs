use std::time::{Duration, Instant};

const FIRST_FRAME_DEADLINE: Duration = Duration::from_secs(2);
const FRESHNESS_WINDOW: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Decision {
    UseStaging,
    UseGdi,
    Demote,
}

pub(crate) fn decide(opened_at: Instant, last_frame_at: Option<Instant>, now: Instant) -> Decision {
    let Some(last_frame_at) = last_frame_at else {
        return before_the_first_frame(now.saturating_duration_since(opened_at));
    };
    let frame_age = now.saturating_duration_since(last_frame_at);
    if frame_age < FRESHNESS_WINDOW { Decision::UseStaging } else { Decision::UseGdi }
}

fn before_the_first_frame(open_for: Duration) -> Decision {
    if open_for < FIRST_FRAME_DEADLINE { Decision::UseGdi } else { Decision::Demote }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(milliseconds: u64) -> Duration {
        Duration::from_millis(milliseconds)
    }

    fn served(staging: &'static str, live: &'static str, decision: Decision) -> &'static str {
        match decision {
            Decision::UseStaging => staging,
            Decision::UseGdi | Decision::Demote => live,
        }
    }

    #[test]
    fn captures_are_live_gdi_until_the_first_real_frame() {
        let opened = Instant::now();
        assert_eq!(decide(opened, None, opened), Decision::UseGdi);
        assert_eq!(decide(opened, None, opened + ms(1999)), Decision::UseGdi);
    }

    #[test]
    fn outputs_without_a_frame_within_two_seconds_are_demoted() {
        let opened = Instant::now();
        assert_eq!(decide(opened, None, opened + ms(2000)), Decision::Demote);
        assert_eq!(decide(opened, None, opened + ms(600_000)), Decision::Demote);
    }

    #[test]
    fn staging_is_reused_only_while_the_last_frame_is_recent() {
        let opened = Instant::now();
        let frame = Some(opened + ms(100));
        assert_eq!(decide(opened, frame, opened + ms(100)), Decision::UseStaging);
        assert_eq!(decide(opened, frame, opened + ms(2099)), Decision::UseStaging);
        assert_eq!(decide(opened, frame, opened + ms(2100)), Decision::UseGdi);
        assert_eq!(decide(opened, frame, opened + ms(60_000)), Decision::UseGdi);
    }

    #[test]
    fn a_live_output_is_never_demoted_once_a_frame_arrived() {
        let opened = Instant::now();
        assert_eq!(decide(opened, Some(opened + ms(1900)), opened + ms(10_000)), Decision::UseGdi);
    }

    #[test]
    fn a_change_dxgi_never_reports_is_seen_by_the_next_capture() {
        let opened = Instant::now();
        let last_frame = Some(opened + ms(10));
        let first = served("before", "before", decide(opened, last_frame, opened + ms(500)));
        let second = served("before", "after", decide(opened, last_frame, opened + ms(2500)));
        assert_eq!((first, second), ("before", "after"));
    }

    #[test]
    fn a_change_before_any_frame_is_seen_by_the_next_capture() {
        let opened = Instant::now();
        let first = served("never filled", "before", decide(opened, None, opened + ms(5)));
        let second = served("never filled", "after", decide(opened, None, opened + ms(40)));
        assert_eq!((first, second), ("before", "after"));
    }

    #[test]
    fn clocks_running_backwards_do_not_panic() {
        let now = Instant::now();
        assert_eq!(decide(now + ms(50), Some(now + ms(60)), now), Decision::UseStaging);
        assert_eq!(decide(now + ms(50), None, now), Decision::UseGdi);
    }
}
