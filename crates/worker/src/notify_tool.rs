use deckhand_core::{
    Result,
    protocol::{NotifyArgs, NotifyReport},
};
use deckhand_desktop::NotifyOutcome;

use crate::{call_scope::CallScope, engine::Engine};

pub fn run(engine: &Engine, args: &NotifyArgs, call: &CallScope) -> Result<NotifyReport> {
    args.validate()?;
    engine.guard.check_running(call)?;
    engine.notifier.notify(&args.title, &args.message).map(report)
}

fn report(outcome: NotifyOutcome) -> NotifyReport {
    match outcome {
        NotifyOutcome::Shown => NotifyReport { shown: true, note: None },
        NotifyOutcome::ShownQuiet(reason) => NotifyReport { shown: false, note: Some(reason.describe().to_string()) },
    }
}

#[cfg(test)]
mod tests {
    use deckhand_core::{ErrorCode, format};
    use deckhand_desktop::QuietReason;

    use super::*;
    use crate::test_fixture::engine;

    fn args(title: &str, message: &str) -> NotifyArgs {
        NotifyArgs { title: title.to_string(), message: message.to_string() }
    }

    #[test]
    fn outcomes_become_reports() {
        assert_eq!(report(NotifyOutcome::Shown), NotifyReport { shown: true, note: None });
        let quiet = report(NotifyOutcome::ShownQuiet(QuietReason::QuietHours));
        assert_eq!(format::notify(&quiet), "notification suppressed by Focus Assist (quiet hours)");
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
