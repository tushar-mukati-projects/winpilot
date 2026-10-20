use std::time::Duration;

use parking_lot::Mutex;
use winpilot_core::Result;
use winpilot_win32::MessageThread;

use crate::{
    app_identity,
    balloon::Balloon,
    balloon_text::BalloonText,
    quiet_state::{QuietReason, current_quiet_reason},
    toast::{ToastBlock, ToastResult, show_toast},
};

const MAX_SHOWING: usize = 4;
const BALLOON_STARTUP: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyOutcome {
    Sent { quiet: Option<QuietReason> },
    Blocked(ToastBlock),
    SentAsBalloon { quiet: Option<QuietReason> },
}

#[derive(Debug, Default)]
pub struct Notifier {
    showing: Mutex<Vec<MessageThread>>,
}

impl Notifier {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn notify(&self, title: &str, message: &str) -> Result<NotifyOutcome> {
        let text = BalloonText::new(title, message)?;
        let quiet = current_quiet_reason();
        if app_identity::is_registered()
            && let Ok(result) = show_toast(title, message)
        {
            return Ok(toast_outcome(result, quiet));
        }
        self.show_balloon(text)?;
        Ok(NotifyOutcome::SentAsBalloon { quiet })
    }

    pub fn dismiss_all(&self) {
        let dismissed = std::mem::take(&mut *self.showing.lock());
        drop(dismissed);
    }

    fn show_balloon(&self, text: BalloonText) -> Result<()> {
        let mut showing = self.showing.lock();
        showing.retain(|balloon| !balloon.is_finished());
        while showing.len() >= MAX_SHOWING {
            showing.remove(0);
        }
        let (balloon, ()) = MessageThread::spawn_within("winpilot-notify", BALLOON_STARTUP, move |cancel| {
            Balloon::show(&text, cancel).map(|shown| (shown, ()))
        })?;
        showing.push(balloon);
        Ok(())
    }
}

const fn toast_outcome(result: ToastResult, quiet: Option<QuietReason>) -> NotifyOutcome {
    match result {
        ToastResult::Sent => NotifyOutcome::Sent { quiet },
        ToastResult::Blocked(block) => NotifyOutcome::Blocked(block),
    }
}

#[cfg(test)]
mod tests {
    use winpilot_core::ErrorCode;

    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn notifier_is_send_and_sync() {
        assert_send_sync::<Notifier>();
    }

    #[test]
    fn toast_results_keep_the_quiet_state_unless_blocked() {
        let quiet = Some(QuietReason::FullScreen);
        assert_eq!(toast_outcome(ToastResult::Sent, quiet), NotifyOutcome::Sent { quiet });
        assert_eq!(
            toast_outcome(ToastResult::Blocked(ToastBlock::ForUser), quiet),
            NotifyOutcome::Blocked(ToastBlock::ForUser)
        );
    }

    #[test]
    fn bad_text_is_refused_before_anything_is_shown() {
        let notifier = Notifier::new();
        let error = notifier.notify(&"t".repeat(64), "Export finished").unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        assert_eq!(notifier.notify("Export", "").unwrap_err().code, ErrorCode::InvalidArgument);
        assert!(notifier.showing.lock().is_empty());
    }

    #[test]
    fn dismissing_with_nothing_shown_is_harmless() {
        let notifier = Notifier::new();
        notifier.dismiss_all();
        assert!(notifier.showing.lock().is_empty());
    }

    #[test]
    #[ignore = "shows a notification"]
    fn shows_a_real_notification_and_removes_any_tray_icon_on_drop() {
        let notifier = Notifier::new();
        let outcome = notifier.notify("WinPilot test", "This is a test notification from cargo test.").unwrap();
        assert!(!matches!(outcome, NotifyOutcome::Blocked(_)), "{outcome:?}");
        notifier.dismiss_all();
        assert!(notifier.showing.lock().is_empty());
    }
}
