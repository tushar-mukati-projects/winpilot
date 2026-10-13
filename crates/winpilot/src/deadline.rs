use std::time::Duration;

use tokio::time::{Instant, Sleep, sleep_until};
use winpilot_core::{Error, ErrorCode};

use crate::death::{Death, seconds};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Deadline {
    tool: &'static str,
    budget: Duration,
    at: Instant,
}

impl Deadline {
    pub fn from_now(tool: &'static str, budget: Duration) -> Self {
        Self { tool, budget, at: Instant::now() + budget }
    }

    pub fn expired(&self) -> Sleep {
        sleep_until(self.at)
    }

    pub fn has_passed(&self) -> bool {
        Instant::now() >= self.at
    }

    pub fn remaining(&self) -> Duration {
        self.at.saturating_duration_since(Instant::now())
    }

    pub const fn death(&self) -> Death {
        Death::Timeout { tool: self.tool, deadline: self.budget }
    }

    pub fn still_starting(&self) -> Error {
        let message = format!(
            "the desktop worker was still starting when {} reached its {} deadline; try again",
            self.tool,
            seconds(self.budget)
        );
        Error::new(ErrorCode::TargetNotResponding, message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadline_is_fixed_when_created() {
        let deadline = Deadline::from_now("observe status", Duration::ZERO);
        assert!(deadline.has_passed());
        assert_eq!(deadline.remaining(), Duration::ZERO);
        let later = Deadline::from_now("observe status", Duration::from_secs(60));
        assert!(!later.has_passed());
        assert!(later.remaining() > Duration::from_secs(59) && later.remaining() <= Duration::from_secs(60));
    }

    #[test]
    fn startup_overrun_names_the_tool_and_budget() {
        let error = Deadline::from_now("observe", Duration::from_millis(2500)).still_starting();
        assert_eq!(error.code, ErrorCode::TargetNotResponding);
        assert_eq!(
            error.message,
            "the desktop worker was still starting when observe reached its 2.5 s deadline; try again"
        );
    }

    #[test]
    fn death_carries_the_whole_budget() {
        let deadline = Deadline::from_now("act", Duration::from_secs(16));
        assert_eq!(deadline.death(), Death::Timeout { tool: "act", deadline: Duration::from_secs(16) });
    }
}
