use deckhand_core::{
    Result,
    protocol::{NotifyArgs, NotifyReport},
};
use deckhand_desktop::{NotifyOutcome, QuietReason};

use crate::{call_scope::CallScope, engine::Engine};

const BALLOON_NOTE: &str = "sent as a tray balloon; Windows may hold it back";

pub fn run(engine: &Engine, args: &NotifyArgs, call: &CallScope) -> Result<NotifyReport> {
    args.validate()?;
    engine.guard.check_running(call)?;
    engine.notifier.notify(&args.title, &args.message).map(report)
}

fn report(outcome: NotifyOutcome) -> NotifyReport {
    match outcome {
        NotifyOutcome::Sent { quiet: None } => NotifyReport { shown: true, note: None },
        NotifyOutcome::Sent { quiet: Some(reason) } => NotifyReport { shown: true, note: Some(quiet_toast(reason)) },
        NotifyOutcome::Blocked(block) => NotifyReport { shown: false, note: Some(block.describe().to_string()) },
        NotifyOutcome::SentAsBalloon { quiet: None } => {
            NotifyReport { shown: true, note: Some(BALLOON_NOTE.to_string()) }
        }
        NotifyOutcome::SentAsBalloon { quiet: Some(reason) } => {
            NotifyReport { shown: false, note: Some(quiet_balloon(reason)) }
        }
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
    use deckhand_core::{ErrorCode, format};
    use deckhand_desktop::ToastBlock;

    use super::*;
    use crate::test_fixture::engine;

    fn args(title: &str, message: &str) -> NotifyArgs {
        NotifyArgs { title: title.to_string(), message: message.to_string() }
    }

    fn text(outcome: NotifyOutcome) -> String {
        format::notify(&report(outcome))
    }

    #[test]
    fn toasts_read_as_sent_and_say_when_windows_may_keep_them_quiet() {
        assert_eq!(text(NotifyOutcome::Sent { quiet: None }), "notification sent");
        assert_eq!(
            text(NotifyOutcome::Sent { quiet: Some(QuietReason::Presentation) }),
            "notification sent; Windows may hold the banner back during presentation mode, but it stays in Notification Center"
        );
    }

    #[test]
    fn blocked_toasts_name_the_setting_and_are_not_shown() {
        let blocked = report(NotifyOutcome::Blocked(ToastBlock::ForApplication));
        assert!(!blocked.shown);
        assert_eq!(
            format::notify(&blocked),
            "notification not shown: notifications from Deckhand are turned off in Settings > System > Notifications"
        );
    }

    #[test]
    fn balloons_are_reported_honestly() {
        assert_eq!(
            text(NotifyOutcome::SentAsBalloon { quiet: None }),
            "notification sent as a tray balloon; Windows may hold it back"
        );
        let quiet = report(NotifyOutcome::SentAsBalloon { quiet: Some(QuietReason::QuietTime) });
        assert!(!quiet.shown);
        assert_eq!(
            format::notify(&quiet),
            "notification not shown: sent as a tray balloon, but Windows holds balloons back during the first hour after sign-in or a Windows upgrade"
        );
    }

    #[test]
    fn invalid_text_never_reaches_the_desktop() {
        let call = CallScope::relaxed();
        let error = run(engine(), &args("Done", &"m".repeat(300)), &call).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        assert!(error.message.starts_with("message must be at most 255 characters"), "{}", error.message);
        assert_eq!(run(engine(), &args(" ", "Export finished"), &call).unwrap_err().code, ErrorCode::InvalidArgument);
        assert_eq!(engine().notifier.showing(), 0);
    }
}
