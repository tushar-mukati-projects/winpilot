use deckhand_core::{
    Error, ErrorCode, WindowInfo,
    format::other_desktop,
    protocol::{ClickHow, MouseAction, MouseButton, Step, TypeHow, WindowAction},
};

const MOUSE: &str = "the real mouse";
const KEYBOARD: &str = "the real keyboard";
const FOCUS: &str = "the user's focus";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    pub step: String,
    pub needs: &'static str,
    pub instead: Option<&'static str>,
}

pub fn background_refusal(step: &Step) -> Option<Refusal> {
    match step {
        Step::Click { button, count, modifiers, how, .. } => click_refusal(*button, *count, modifiers.is_some(), *how),
        Step::Type { target, enter, how, .. } => type_refusal(target.is_some(), *enter, *how),
        Step::Key { .. } => Some(refused("key", KEYBOARD, Some("use set"))),
        Step::Mouse { action, target, .. } => mouse_refusal(*action, target.is_some()),
        Step::Focus { .. } => Some(refused("focus", FOCUS, None)),
        Step::Window { action, .. } => window_refusal(*action),
        Step::Pick { .. } => Some(refused("pick", "a file dialog in the foreground", None)),
        Step::Set { .. } | Step::Wait(_) | Step::Clipboard { .. } | Step::Notify { .. } => None,
    }
}

impl Refusal {
    pub fn in_background(&self) -> Error {
        let instead = self.instead.map(|instead| format!(" or {instead}")).unwrap_or_default();
        let message =
            format!("{} needs {}; background mode never touches it (drop background{instead})", self.step, self.needs);
        Error::new(ErrorCode::NeedsUser, message)
    }

    pub fn on_other_desktop(&self, window: &WindowInfo) -> Error {
        let desktop = other_desktop(window).unwrap_or_else(|| "another desktop".to_string());
        let instead = self.instead.map(|instead| format!("{instead}, or ")).unwrap_or_default();
        let message = format!(
            "{} is on {desktop}, where mouse and keyboard input can't reach and Deckhand never switches; {} needs {} ({instead}move the window to this desktop)",
            window.hwnd, self.step, self.needs
        );
        Error::new(ErrorCode::NeedsUser, message)
    }
}

fn refused(step: impl Into<String>, needs: &'static str, instead: Option<&'static str>) -> Refusal {
    Refusal { step: step.into(), needs, instead }
}

fn click_refusal(button: MouseButton, count: u8, modified: bool, how: ClickHow) -> Option<Refusal> {
    if how == ClickHow::Mouse {
        return Some(refused("click how=mouse", MOUSE, Some("use how=auto")));
    }
    if modified {
        return Some(refused("a click with modifiers", "the real mouse and keyboard", None));
    }
    if button != MouseButton::Left {
        return Some(refused(format!("a {} click", button.name()), MOUSE, None));
    }
    (count > 1).then(|| refused("a double or triple click", MOUSE, None))
}

fn type_refusal(targeted: bool, enter: bool, how: TypeHow) -> Option<Refusal> {
    if !targeted {
        return Some(refused("type without a target", "the keyboard focus", Some("give the field as target")));
    }
    match how {
        TypeHow::Keys => return Some(refused("type how=keys", KEYBOARD, Some("use how=auto"))),
        TypeHow::Paste => return Some(refused("type how=paste", "the clipboard and keyboard", Some("use how=auto"))),
        TypeHow::Auto => {}
    }
    enter.then(|| refused("type with enter", KEYBOARD, Some("drop enter and click the submit button")))
}

fn mouse_refusal(action: MouseAction, targeted: bool) -> Option<Refusal> {
    match action {
        MouseAction::Scroll if targeted => None,
        MouseAction::Scroll => {
            Some(refused("mouse scroll without a target", MOUSE, Some("give the scrollable element as target")))
        }
        MouseAction::Move | MouseAction::Down | MouseAction::Up | MouseAction::Drag => {
            Some(refused(format!("mouse {}", action.name()), MOUSE, None))
        }
    }
}

fn window_refusal(action: WindowAction) -> Option<Refusal> {
    match action {
        WindowAction::Close => None,
        WindowAction::Launch
        | WindowAction::Minimize
        | WindowAction::Maximize
        | WindowAction::Restore
        | WindowAction::Move => Some(refused(format!("window {}", action.name()), FOCUS, None)),
    }
}

#[cfg(test)]
mod tests {
    use deckhand_core::Rect;

    use super::*;
    use crate::test_fixture::window;

    fn step(json: &str) -> Step {
        serde_json::from_str(json).unwrap()
    }

    fn refusal(json: &str) -> Option<String> {
        background_refusal(&step(json)).map(|refusal| refusal.in_background().message)
    }

    #[test]
    fn pattern_steps_run_in_the_background() {
        for json in [
            r#"{"do":"click","target":"e1"}"#,
            r#"{"do":"click","target":"e1","how":"invoke"}"#,
            r#"{"do":"type","target":"e1","text":"hi"}"#,
            r#"{"do":"set","target":"e1","checked":true}"#,
            r#"{"do":"mouse","action":"scroll","target":"e1","direction":"down"}"#,
            r#"{"do":"window","action":"close","target":"w1"}"#,
            r#"{"do":"wait","ms":10}"#,
            r#"{"do":"clipboard","text":"x"}"#,
            r#"{"do":"notify","title":"a","message":"b"}"#,
        ] {
            assert_eq!(refusal(json), None, "{json}");
        }
    }

    #[test]
    fn real_input_steps_are_refused_with_a_way_out() {
        let cases = [
            (
                r#"{"do":"key","keys":"ctrl+s"}"#,
                "key needs the real keyboard; background mode never touches it (drop background or use set)",
            ),
            (
                r#"{"do":"focus","target":"e1"}"#,
                "focus needs the user's focus; background mode never touches it (drop background)",
            ),
            (
                r#"{"do":"mouse","action":"drag","target":"e1","to":"e2"}"#,
                "mouse drag needs the real mouse; background mode never touches it (drop background)",
            ),
            (
                r#"{"do":"mouse","action":"move","target":"e1"}"#,
                "mouse move needs the real mouse; background mode never touches it (drop background)",
            ),
            (
                r#"{"do":"pick","paths":["C:\\a.txt"]}"#,
                "pick needs a file dialog in the foreground; background mode never touches it (drop background)",
            ),
            (
                r#"{"do":"click","target":"e1","button":"right"}"#,
                "a right click needs the real mouse; background mode never touches it (drop background)",
            ),
            (
                r#"{"do":"click","target":"e1","count":2}"#,
                "a double or triple click needs the real mouse; background mode never touches it (drop background)",
            ),
            (
                r#"{"do":"type","text":"hi"}"#,
                "type without a target needs the keyboard focus; background mode never touches it (drop background or give the field as target)",
            ),
            (
                r#"{"do":"type","target":"e1","text":"hi","enter":true}"#,
                "type with enter needs the real keyboard; background mode never touches it (drop background or drop enter and click the submit button)",
            ),
            (
                r#"{"do":"window","action":"launch","app":"Notepad"}"#,
                "window launch needs the user's focus; background mode never touches it (drop background)",
            ),
        ];
        for (json, expected) in cases {
            assert_eq!(refusal(json).as_deref(), Some(expected), "{json}");
        }
        assert!(refusal(r#"{"do":"click","target":"e1","modifiers":"ctrl"}"#).is_some());
        assert!(refusal(r#"{"do":"click","target":"e1","how":"mouse"}"#).is_some());
        assert!(refusal(r#"{"do":"type","target":"e1","text":"x","how":"paste"}"#).is_some());
        assert!(refusal(r#"{"do":"mouse","action":"scroll","direction":"up"}"#).is_some());
        assert!(refusal(r#"{"do":"window","action":"maximize","target":"w1"}"#).is_some());
    }

    #[test]
    fn refusals_are_needs_user_errors() {
        let error = background_refusal(&step(r#"{"do":"key","keys":"a"}"#)).unwrap().in_background();
        assert_eq!(error.code, ErrorCode::NeedsUser);
    }

    #[test]
    fn other_desktop_refusals_name_the_desktop() {
        let elsewhere = WindowInfo {
            desktop: Some(2),
            on_current_desktop: false,
            ..window(123, "notepad.exe", "Notes", Rect::new(0, 0, 10, 10))
        };
        let refusal = background_refusal(&step(r#"{"do":"key","keys":"ctrl+s","target":"w123"}"#)).unwrap();
        let error = refusal.on_other_desktop(&elsewhere);
        assert_eq!(error.code, ErrorCode::NeedsUser);
        assert_eq!(
            error.message,
            "w123 is on desktop 2, where mouse and keyboard input can't reach and Deckhand never switches; key needs the real keyboard (use set, or move the window to this desktop)"
        );
        let focus = background_refusal(&step(r#"{"do":"focus","target":"w123"}"#)).unwrap();
        let unnumbered = WindowInfo { desktop: None, ..elsewhere };
        assert!(focus.on_other_desktop(&unnumbered).message.starts_with("w123 is on another desktop, "));
        assert!(focus.on_other_desktop(&unnumbered).message.ends_with("(move the window to this desktop)"));
    }
}
