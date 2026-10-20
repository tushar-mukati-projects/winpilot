use winpilot_core::{
    Error, ErrorCode, WindowInfo,
    format::other_desktop,
    keys::parse_keys,
    protocol::{ClickHow, KeyAction, MouseAction, Step, TypeHow},
};

const MOUSE: &str = "the real mouse";
const KEYBOARD: &str = "the real keyboard";
const MODIFIER_STATE: &str = "the real keyboard, because apps read ctrl, alt, shift and win from it";
const FOCUS: &str = "the user's focus";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refusal {
    pub(crate) step: String,
    pub(crate) needs: &'static str,
    pub(crate) instead: Option<&'static str>,
}

pub(crate) fn background_refusal(step: &Step) -> Option<Refusal> {
    match step {
        Step::Click { modifiers, how, .. } => click_refusal(modifiers.is_some(), *how),
        Step::Type { target, how, .. } => type_refusal(target.is_some(), *how),
        Step::Key { keys, target, action, .. } => key_refusal(keys, target.is_some(), *action),
        Step::Mouse { action, target, .. } => mouse_refusal(*action, target.is_some()),
        Step::Focus { .. } => Some(refused("focus", FOCUS, None)),
        Step::Window { .. }
        | Step::Pick { .. }
        | Step::Set { .. }
        | Step::Wait(_)
        | Step::Clipboard { .. }
        | Step::Notify { .. } => None,
    }
}

impl Refusal {
    pub(crate) fn in_background(&self) -> Error {
        let instead = self.instead.map(|instead| format!(" or {instead}")).unwrap_or_default();
        let message =
            format!("{} needs {}; background mode never touches it (drop background{instead})", self.step, self.needs);
        Error::new(ErrorCode::NeedsUser, message)
    }

    pub(crate) fn on_other_desktop(&self, window: &WindowInfo) -> Error {
        let desktop = other_desktop(window).unwrap_or_else(|| "another desktop".to_string());
        let instead = self.instead.map(|instead| format!("{instead}, or ")).unwrap_or_default();
        let message = format!(
            "{} is on {desktop}, where mouse and keyboard input can't reach and WinPilot never switches; {} needs {} ({instead}move the window to this desktop)",
            window.hwnd, self.step, self.needs
        );
        Error::new(ErrorCode::NeedsUser, message)
    }
}

fn refused(step: impl Into<String>, needs: &'static str, instead: Option<&'static str>) -> Refusal {
    Refusal { step: step.into(), needs, instead }
}

fn click_refusal(modified: bool, how: ClickHow) -> Option<Refusal> {
    if how == ClickHow::Mouse {
        return Some(refused("click how=mouse", MOUSE, Some("use how=auto")));
    }
    modified.then(|| refused("a click with modifiers", MODIFIER_STATE, None))
}

fn type_refusal(targeted: bool, how: TypeHow) -> Option<Refusal> {
    if !targeted {
        return Some(refused("type without a target", "the keyboard focus", Some("give the field as target")));
    }
    (how == TypeHow::Paste).then(|| refused("type how=paste", "the clipboard and keyboard", Some("use how=auto")))
}

fn key_refusal(keys: &str, targeted: bool, action: KeyAction) -> Option<Refusal> {
    if action != KeyAction::Press {
        return Some(refused(format!("key {}", action.name()), KEYBOARD, None));
    }
    if has_modifiers(keys) {
        return Some(refused("a key chord with modifiers", MODIFIER_STATE, None));
    }
    (!targeted)
        .then(|| refused("key without a target", "the keyboard focus", Some("give the window or field as target")))
}

fn has_modifiers(keys: &str) -> bool {
    parse_keys(keys).is_ok_and(|chords| chords.iter().any(|chord| !chord.modifiers.is_empty()))
}

fn mouse_refusal(action: MouseAction, targeted: bool) -> Option<Refusal> {
    match action {
        MouseAction::Scroll | MouseAction::Drag if targeted => None,
        MouseAction::Scroll => {
            Some(refused("mouse scroll without a target", MOUSE, Some("give the element or point to scroll as target")))
        }
        MouseAction::Drag => Some(refused("mouse drag without a target", MOUSE, Some("give where to grab as target"))),
        MouseAction::Down | MouseAction::Up => {
            Some(refused(format!("mouse {}", action.name()), MOUSE, Some("use click or mouse drag")))
        }
        MouseAction::Move => Some(refused("mouse move", MOUSE, None)),
    }
}

#[cfg(test)]
mod tests {
    use winpilot_core::Rect;

    use super::*;
    use crate::test_fixture::{step, window};

    fn refusal(json: &str) -> Option<String> {
        background_refusal(&step(json)).map(|refusal| refusal.in_background().message)
    }

    #[test]
    fn pattern_and_message_steps_run_in_the_background() {
        for json in [
            r#"{"do":"click","target":"e1"}"#,
            r#"{"do":"click","target":"e1","how":"invoke"}"#,
            r#"{"do":"click","target":"e1","button":"right"}"#,
            r#"{"do":"click","target":"e1","count":2}"#,
            r#"{"do":"click","target":{"x":5,"y":5,"img":"i1"},"count":3,"button":"middle"}"#,
            r#"{"do":"type","target":"e1","text":"hi"}"#,
            r#"{"do":"type","target":"e1","text":"hi","enter":true}"#,
            r#"{"do":"type","target":"e1","text":"hi","how":"keys"}"#,
            r#"{"do":"key","keys":"end","target":"e1"}"#,
            r#"{"do":"key","keys":"enter backspace left","target":"w1","repeat":2}"#,
            r#"{"do":"set","target":"e1","checked":true}"#,
            r#"{"do":"mouse","action":"scroll","target":"e1","direction":"down"}"#,
            r#"{"do":"mouse","action":"drag","target":"e1","to":"e2"}"#,
            r#"{"do":"window","action":"close","target":"w1"}"#,
            r#"{"do":"window","action":"launch","app":"Notepad"}"#,
            r#"{"do":"window","action":"minimize","target":"w1"}"#,
            r#"{"do":"window","action":"maximize","target":"w1"}"#,
            r#"{"do":"window","action":"restore","target":"w1"}"#,
            r#"{"do":"window","action":"move","target":"w1","rect":{"x":0,"y":0,"width":10,"height":10}}"#,
            r#"{"do":"pick","paths":["C:\\a.txt"]}"#,
            r#"{"do":"pick","paths":["C:\\a.txt"],"open_with":"e1","overwrite":true}"#,
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
                r#"{"do":"key","keys":"ctrl+s","target":"w1"}"#,
                "a key chord with modifiers needs the real keyboard, because apps read ctrl, alt, shift and win from it; background mode never touches it (drop background)",
            ),
            (
                r#"{"do":"key","keys":"enter"}"#,
                "key without a target needs the keyboard focus; background mode never touches it (drop background or give the window or field as target)",
            ),
            (
                r#"{"do":"key","keys":"shift","target":"w1","action":"down"}"#,
                "key down needs the real keyboard; background mode never touches it (drop background)",
            ),
            (
                r#"{"do":"focus","target":"e1"}"#,
                "focus needs the user's focus; background mode never touches it (drop background)",
            ),
            (
                r#"{"do":"mouse","action":"move","target":"e1"}"#,
                "mouse move needs the real mouse; background mode never touches it (drop background)",
            ),
            (
                r#"{"do":"mouse","action":"down","target":"e1"}"#,
                "mouse down needs the real mouse; background mode never touches it (drop background or use click or mouse drag)",
            ),
            (
                r#"{"do":"click","target":"e1","modifiers":"ctrl"}"#,
                "a click with modifiers needs the real keyboard, because apps read ctrl, alt, shift and win from it; background mode never touches it (drop background)",
            ),
            (
                r#"{"do":"type","text":"hi"}"#,
                "type without a target needs the keyboard focus; background mode never touches it (drop background or give the field as target)",
            ),
        ];
        for (json, expected) in cases {
            assert_eq!(refusal(json).as_deref(), Some(expected), "{json}");
        }
        assert!(refusal(r#"{"do":"click","target":"e1","how":"mouse"}"#).is_some());
        assert!(refusal(r#"{"do":"type","target":"e1","text":"x","how":"paste"}"#).is_some());
        assert!(refusal(r#"{"do":"mouse","action":"scroll","direction":"up"}"#).is_some());
        assert!(refusal(r#"{"do":"mouse","action":"drag","to":"e2"}"#).is_some());
        assert!(refusal(r#"{"do":"key","keys":"end ctrl+a","target":"w1"}"#).is_some());
    }

    #[test]
    fn unparsable_keys_are_left_to_validation() {
        assert!(!has_modifiers("ctrl+"));
        assert!(has_modifiers("a alt+f4"));
        assert!(!has_modifiers("a b enter"));
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
        let refusal = background_refusal(&step(r#"{"do":"mouse","action":"move","target":"w123"}"#)).unwrap();
        let error = refusal.on_other_desktop(&elsewhere);
        assert_eq!(error.code, ErrorCode::NeedsUser);
        assert_eq!(
            error.message,
            "w123 is on desktop 2, where mouse and keyboard input can't reach and WinPilot never switches; mouse move needs the real mouse (move the window to this desktop)"
        );
        let typing = background_refusal(&step(r#"{"do":"type","text":"x"}"#)).unwrap();
        assert!(
            typing
                .on_other_desktop(&elsewhere)
                .message
                .ends_with("(give the field as target, or move the window to this desktop)")
        );
        let focus = background_refusal(&step(r#"{"do":"focus","target":"w123"}"#)).unwrap();
        let unnumbered = WindowInfo { desktop: None, ..elsewhere };
        assert!(focus.on_other_desktop(&unnumbered).message.starts_with("w123 is on another desktop, "));
        assert!(focus.on_other_desktop(&unnumbered).message.ends_with("(move the window to this desktop)"));
    }
}
