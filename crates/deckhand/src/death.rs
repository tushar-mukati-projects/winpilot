use std::time::Duration;

use deckhand_core::{Error, ErrorCode};

const SHUTTING_DOWN: &str = "the deckhand server is shutting down";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Death {
    Timeout { tool: &'static str, deadline: Duration },
    Exited,
    Unreadable(String),
    Unwritable(String),
    Shutdown,
}

impl Death {
    pub fn error(&self) -> Error {
        match self.cause() {
            Some(cause) => Error::new(ErrorCode::TargetNotResponding, format!("{cause}; it was restarted")),
            None => Error::new(ErrorCode::Cancelled, SHUTTING_DOWN),
        }
    }

    pub const fn counts_as_restart(&self) -> bool {
        !matches!(self, Self::Shutdown)
    }

    fn cause(&self) -> Option<String> {
        let cause = match self {
            Self::Timeout { tool, deadline } => {
                format!("the desktop worker did not answer {tool} within {} (an app may be hung)", seconds(*deadline))
            }
            Self::Exited => "the desktop worker exited unexpectedly".to_string(),
            Self::Unreadable(detail) => format!("the desktop worker sent an unreadable message ({detail})"),
            Self::Unwritable(detail) => format!("the desktop worker stopped accepting calls ({detail})"),
            Self::Shutdown => return None,
        };
        Some(cause)
    }
}

pub fn seconds(duration: Duration) -> String {
    if duration.subsec_millis() == 0 {
        format!("{} s", duration.as_secs())
    } else {
        format!("{:.1} s", duration.as_secs_f64())
    }
}

pub fn stopped_error() -> Error {
    Error::new(ErrorCode::TargetNotResponding, "the desktop worker stopped before answering; try again")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_message_names_the_tool_and_deadline() {
        let death = Death::Timeout { tool: "observe", deadline: Duration::from_secs(10) };
        let error = death.error();
        assert_eq!(error.code, ErrorCode::TargetNotResponding);
        assert_eq!(
            error.message,
            "the desktop worker did not answer observe within 10 s (an app may be hung); it was restarted"
        );
    }

    #[test]
    fn every_worker_death_says_it_was_restarted() {
        let deaths = [Death::Exited, Death::Unreadable("bad".to_string()), Death::Unwritable("pipe".to_string())];
        for death in deaths {
            let error = death.error();
            assert_eq!(error.code, ErrorCode::TargetNotResponding);
            assert!(error.message.ends_with("; it was restarted"), "{}", error.message);
        }
    }

    #[test]
    fn fractional_deadlines_show_one_decimal() {
        assert_eq!(seconds(Duration::from_millis(5500)), "5.5 s");
        assert_eq!(seconds(Duration::from_secs(3)), "3 s");
    }

    #[test]
    fn shutdown_is_not_a_restart() {
        assert!(!Death::Shutdown.counts_as_restart());
        assert!(Death::Exited.counts_as_restart());
        let error = Death::Shutdown.error();
        assert_eq!(error.code, ErrorCode::Cancelled);
        assert_eq!(error.message, SHUTTING_DOWN);
    }
}
