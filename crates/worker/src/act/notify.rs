use std::time::Instant;

use deckhand_core::Result;
use deckhand_desktop::{NotifyOutcome, QuietReason};

use super::{done::Done, steps::StepContext};
use crate::engine::Engine;

const BALLOON_NOTE: &str = "sent as a tray balloon; Windows may hold it back";

pub(crate) fn step(engine: &Engine, title: &str, message: &str, context: &StepContext<'_>) -> Result<Done> {
    engine.guard.check_running(context.call)?;
    engine.notify_limit.lock().admit(context.started, Instant::now())?;
    engine.notifier.notify(title, message).map(|outcome| Done::noting(outcome_note(outcome)))
}

fn outcome_note(outcome: NotifyOutcome) -> String {
    match outcome {
        NotifyOutcome::Sent { quiet: None } => "sent".to_string(),
        NotifyOutcome::Sent { quiet: Some(reason) } => quiet_toast(reason),
        NotifyOutcome::Blocked(block) => format!("not shown: {}", block.describe()),
        NotifyOutcome::SentAsBalloon { quiet: None } => BALLOON_NOTE.to_string(),
        NotifyOutcome::SentAsBalloon { quiet: Some(reason) } => format!("not shown: {}", quiet_balloon(reason)),
    }
}

fn quiet_toast(reason: QuietReason) -> String {
    format!("sent; Windows may hold the banner back during {}, but it stays in Notification Center", reason.describe())
}

fn quiet_balloon(reason: QuietReason) -> String {
    format!("sent as a tray balloon, but Windows holds balloons back during {}", reason.describe())
}

#[cfg(test)]
mod tests {
    use deckhand_desktop::ToastBlock;

    use super::*;

    #[test]
    fn toasts_read_as_sent_and_say_when_windows_may_keep_them_quiet() {
        assert_eq!(outcome_note(NotifyOutcome::Sent { quiet: None }), "sent");
        assert_eq!(
            outcome_note(NotifyOutcome::Sent { quiet: Some(QuietReason::Presentation) }),
            "sent; Windows may hold the banner back during presentation mode, but it stays in Notification Center"
        );
    }

    #[test]
    fn blocked_toasts_name_the_setting() {
        assert_eq!(
            outcome_note(NotifyOutcome::Blocked(ToastBlock::ForApplication)),
            "not shown: notifications from Deckhand are turned off in Settings > System > Notifications"
        );
    }

    #[test]
    fn balloons_are_reported_honestly() {
        assert_eq!(
            outcome_note(NotifyOutcome::SentAsBalloon { quiet: None }),
            "sent as a tray balloon; Windows may hold it back"
        );
        assert_eq!(
            outcome_note(NotifyOutcome::SentAsBalloon { quiet: Some(QuietReason::QuietTime) }),
            "not shown: sent as a tray balloon, but Windows holds balloons back during the first hour after sign-in or a Windows upgrade"
        );
    }
}
