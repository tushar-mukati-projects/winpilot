use deckhand_core::Result;
use parking_lot::Mutex;

use crate::{
    balloon::Balloon,
    balloon_text::BalloonText,
    message_thread::MessageThread,
    quiet_state::{QuietReason, current_quiet_reason},
};

const MAX_SHOWING: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotifyOutcome {
    Shown,
    ShownQuiet(QuietReason),
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
        let mut showing = self.showing.lock();
        showing.retain(|balloon| !balloon.is_finished());
        while showing.len() >= MAX_SHOWING {
            showing.remove(0);
        }
        let (balloon, ()) =
            MessageThread::spawn("deckhand-notify", move || Balloon::show(&text).map(|shown| (shown, ())))?;
        showing.push(balloon);
        Ok(outcome(current_quiet_reason()))
    }

    pub fn showing(&self) -> usize {
        self.showing.lock().iter().filter(|balloon| !balloon.is_finished()).count()
    }
}

const fn outcome(quiet: Option<QuietReason>) -> NotifyOutcome {
    match quiet {
        Some(reason) => NotifyOutcome::ShownQuiet(reason),
        None => NotifyOutcome::Shown,
    }
}

#[cfg(test)]
mod tests {
    use deckhand_core::ErrorCode;

    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn notifier_is_send_and_sync() {
        assert_send_sync::<Notifier>();
    }

    #[test]
    fn quiet_states_become_shown_quiet() {
        assert_eq!(outcome(None), NotifyOutcome::Shown);
        assert_eq!(outcome(Some(QuietReason::QuietHours)), NotifyOutcome::ShownQuiet(QuietReason::QuietHours));
    }

    #[test]
    fn bad_text_is_refused_before_any_icon_is_added() {
        let notifier = Notifier::new();
        let error = notifier.notify(&"t".repeat(64), "Export finished").unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        assert_eq!(notifier.notify("Export", "").unwrap_err().code, ErrorCode::InvalidArgument);
        assert_eq!(notifier.showing(), 0);
    }

    #[test]
    #[ignore = "shows a notification"]
    fn shows_a_real_notification_and_removes_the_icon_on_drop() {
        let notifier = Notifier::new();
        let outcome = notifier.notify("Deckhand test", "This is a test notification from cargo test.").unwrap();
        assert!(matches!(outcome, NotifyOutcome::Shown | NotifyOutcome::ShownQuiet(_)));
        assert_eq!(notifier.showing(), 1);
        drop(notifier);
    }
}
