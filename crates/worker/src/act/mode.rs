use deckhand_core::{Result, WindowInfo, protocol::Step};
use deckhand_desktop::Activation;

use super::refusal::background_refusal;
use crate::{call_scope::CallScope, engine::Engine};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Foreground,
    Background,
}

impl Mode {
    pub(crate) const fn requested(background: bool) -> Self {
        if background { Self::Background } else { Self::Foreground }
    }

    pub(crate) const fn activation(self) -> Activation {
        match self {
            Self::Foreground => Activation::Activate,
            Self::Background => Activation::KeepForeground,
        }
    }
}

pub(crate) fn check_before(engine: &Engine, window: Option<&WindowInfo>, mode: Mode, call: &CallScope) -> Result<()> {
    match mode {
        Mode::Foreground => engine.guard.before_input(window, call),
        Mode::Background => engine.guard.before_patterns(window, call),
    }
}

pub(crate) fn for_window(requested: Mode, window: &WindowInfo, step: &Step) -> Result<Mode> {
    if requested == Mode::Background || window.on_current_desktop {
        return Ok(requested);
    }
    match background_refusal(step) {
        Some(refusal) => Err(refusal.on_other_desktop(window)),
        None => Ok(Mode::Background),
    }
}

#[cfg(test)]
mod tests {
    use deckhand_core::{ErrorCode, Rect};

    use super::*;
    use crate::test_fixture::window;

    fn step(json: &str) -> Step {
        serde_json::from_str(json).unwrap()
    }

    fn here() -> WindowInfo {
        window(5, "app.exe", "Here", Rect::new(0, 0, 10, 10))
    }

    fn elsewhere() -> WindowInfo {
        WindowInfo { desktop: Some(3), on_current_desktop: false, ..here() }
    }

    #[test]
    fn the_current_desktop_keeps_the_requested_mode() {
        let click = step(r#"{"do":"click","target":"w5"}"#);
        assert_eq!(for_window(Mode::Foreground, &here(), &click).unwrap(), Mode::Foreground);
        assert_eq!(for_window(Mode::Background, &here(), &click).unwrap(), Mode::Background);
    }

    #[test]
    fn another_desktop_always_gets_background_behaviour() {
        let click = step(r#"{"do":"click","target":"w5"}"#);
        assert_eq!(for_window(Mode::Foreground, &elsewhere(), &click).unwrap(), Mode::Background);
        let set = step(r#"{"do":"set","target":"w5","value":"x"}"#);
        assert_eq!(for_window(Mode::Foreground, &elsewhere(), &set).unwrap(), Mode::Background);
    }

    #[test]
    fn steps_that_need_real_input_are_refused_there() {
        let typed = step(r#"{"do":"key","keys":"enter","target":"w5"}"#);
        assert_eq!(for_window(Mode::Foreground, &elsewhere(), &typed).unwrap(), Mode::Background);
        let key = step(r#"{"do":"key","keys":"ctrl+s","target":"w5"}"#);
        let error = for_window(Mode::Foreground, &elsewhere(), &key).unwrap_err();
        assert_eq!(error.code, ErrorCode::NeedsUser);
        assert!(error.message.starts_with("w5 is on desktop 3, "), "{}", error.message);
        assert!(for_window(Mode::Foreground, &here(), &key).is_ok());
    }

    #[test]
    fn the_flag_picks_the_mode() {
        assert_eq!(Mode::requested(true), Mode::Background);
        assert_eq!(Mode::requested(false), Mode::Foreground);
    }

    #[test]
    fn only_background_mode_keeps_the_users_foreground() {
        assert_eq!(Mode::Background.activation(), Activation::KeepForeground);
        assert_eq!(Mode::Foreground.activation(), Activation::Activate);
    }

    #[test]
    fn window_actions_and_picks_on_another_desktop_run_in_the_background() {
        for json in [
            r#"{"do":"window","action":"move","target":"w5","rect":{"x":0,"y":0,"width":10,"height":10}}"#,
            r#"{"do":"window","action":"maximize","target":"w5"}"#,
            r#"{"do":"pick","paths":["C:\\a.txt"],"open_with":"w5"}"#,
        ] {
            assert_eq!(for_window(Mode::Foreground, &elsewhere(), &step(json)).unwrap(), Mode::Background, "{json}");
        }
    }
}
