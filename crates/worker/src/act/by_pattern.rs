use std::time::Instant;

use deckhand_core::{Result, WindowInfo, WindowRef};

use super::{foreground::require_no_dialog_on_top, settle};
use crate::{call_scope::CallScope, engine::Engine, guard::Guard};

struct KeptForeground<'a> {
    guard: &'a Guard,
    previous: Option<WindowRef>,
    target: WindowRef,
    since: Instant,
}

pub(crate) fn by_pattern<T>(
    engine: &Engine,
    window: &WindowInfo,
    call: &CallScope,
    action: impl FnOnce() -> Result<T>,
) -> Result<(T, Option<String>)> {
    require_no_dialog_on_top(window.hwnd)?;
    let kept = KeptForeground::record(&engine.guard, window.hwnd);
    let acted = action();
    if acted.is_ok() {
        settle(engine, window, call);
    }
    let handed_back = kept.hand_back();
    acted.map(|value| (value, handed_back))
}

impl<'a> KeptForeground<'a> {
    fn record(guard: &'a Guard, target: WindowRef) -> Self {
        Self { guard, previous: deckhand_desktop::foreground_window(), target, since: Instant::now() }
    }

    fn hand_back(&self) -> Option<String> {
        let previous = self.previous.filter(|previous| *previous != self.target)?;
        let taken = deckhand_desktop::foreground_window() == Some(self.target);
        if !taken || self.guard.physical_input_since(self.since) {
            return None;
        }
        Some(handed_back_note(self.target, previous, deckhand_desktop::focus_window(previous)))
    }
}

fn handed_back_note(target: WindowRef, previous: WindowRef, handed_back: Result<()>) -> String {
    match handed_back {
        Ok(()) => format!("{target} took the foreground, so Deckhand gave it back to {previous}"),
        Err(error) => {
            format!("{target} took the foreground and Deckhand could not give it back to {previous}: {}", error.message)
        }
    }
}

#[cfg(test)]
mod tests {
    use deckhand_core::{Error, ErrorCode};

    use super::*;
    use crate::test_fixture::engine;

    #[test]
    fn the_note_says_whether_the_foreground_was_given_back() {
        assert_eq!(
            handed_back_note(WindowRef(5), WindowRef(9), Ok(())),
            "w5 took the foreground, so Deckhand gave it back to w9"
        );
        let refused = Err(Error::new(ErrorCode::NeedsUser, "Windows refused"));
        assert_eq!(
            handed_back_note(WindowRef(5), WindowRef(9), refused),
            "w5 took the foreground and Deckhand could not give it back to w9: Windows refused"
        );
    }

    #[test]
    fn nothing_is_handed_back_when_the_target_was_already_in_front() {
        let guard = &engine().guard;
        let kept = KeptForeground { guard, previous: Some(WindowRef(5)), target: WindowRef(5), since: Instant::now() };
        assert_eq!(kept.hand_back(), None);
        let nobody = KeptForeground { previous: None, ..kept };
        assert_eq!(nobody.hand_back(), None);
    }
}
