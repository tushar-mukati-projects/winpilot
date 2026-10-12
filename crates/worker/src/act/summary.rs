use deckhand_core::{
    TargetSpec,
    protocol::{KeyAction, MouseAction, MouseButton, SetChange, Step, Until, WaitArgs, WindowAction},
};

use super::clip::{clipped, quoted};

const TEXT_LIMIT: usize = 24;
const TARGET_LIMIT: usize = 60;

pub(crate) fn summarize(step: &Step) -> String {
    match step {
        Step::Click { target, button, count, modifiers, .. } => {
            click_summary(target, *button, *count, modifiers.as_deref())
        }
        Step::Type { target, text, enter, .. } => type_summary(target.as_ref(), text, *enter),
        Step::Key { keys, target, repeat, action } => key_summary(keys, target.as_ref(), *repeat, *action),
        Step::Mouse { action, target, to, button, direction, amount, .. } => {
            let place = target.as_ref().map(|target| format!(" at {}", label(target))).unwrap_or_default();
            match action {
                MouseAction::Scroll => scroll_summary(*direction, amount.unwrap_or(3), &place),
                MouseAction::Drag => drag_summary(target.as_ref(), to.as_ref()),
                MouseAction::Move | MouseAction::Down | MouseAction::Up => {
                    format!("{}{place}", mouse_verb(*action, button.unwrap_or_default()))
                }
            }
        }
        Step::Set { target, .. } => {
            step.set_change().map_or_else(|_| format!("set {}", label(target)), |change| set_summary(target, &change))
        }
        Step::Focus { target } => format!("focus {}", label(target)),
        Step::Window { action, target, app, .. } => window_summary(*action, target.as_ref(), app.as_deref()),
        Step::Pick { paths, .. } => format!("pick {}", file_count(paths.len())),
        Step::Wait(args) => wait_summary(args),
        Step::Clipboard { text } => format!("set the clipboard to {}", quoted(text, TEXT_LIMIT)),
        Step::Notify { title, .. } => format!("notify {}", quoted(title, TEXT_LIMIT)),
    }
}

fn label(target: &TargetSpec) -> String {
    clipped(&target.to_string(), TARGET_LIMIT)
}

fn mouse_verb(action: MouseAction, button: MouseButton) -> String {
    match (action, button) {
        (MouseAction::Move, _) => "mouse move".to_string(),
        (_, MouseButton::Left) => format!("mouse {}", action.name()),
        (_, button) => format!("{} mouse {}", button.name(), action.name()),
    }
}

fn click_summary(target: &TargetSpec, button: MouseButton, count: u8, modifiers: Option<&str>) -> String {
    let held = modifiers.map(str::trim).filter(|modifiers| !modifiers.is_empty());
    let held = held.map(|modifiers| format!(" with {modifiers}")).unwrap_or_default();
    format!("{} {}{held}", click_verb(button, count), label(target))
}

fn click_verb(button: MouseButton, count: u8) -> String {
    let clicks = match count {
        ..=1 => "click",
        2 => "double-click",
        _ => "triple-click",
    };
    match button {
        MouseButton::Left => clicks.to_string(),
        MouseButton::Right => format!("right-{clicks}"),
        MouseButton::Middle => format!("middle-{clicks}"),
    }
}

fn type_summary(target: Option<&TargetSpec>, text: &str, enter: bool) -> String {
    let into = target.map(|target| format!(" into {}", label(target))).unwrap_or_default();
    let enter = if enter { " + enter" } else { "" };
    format!("type {}{into}{enter}", quoted(text, TEXT_LIMIT))
}

fn key_summary(keys: &str, target: Option<&TargetSpec>, repeat: u32, action: KeyAction) -> String {
    let verb = match action {
        KeyAction::Press => "key",
        KeyAction::Down => "key down",
        KeyAction::Up => "key up",
    };
    let inside = target.map(|target| format!(" in {}", label(target))).unwrap_or_default();
    let times = if repeat > 1 { format!(" ×{repeat}") } else { String::new() };
    format!("{verb} {}{inside}{times}", clipped(keys.trim(), TARGET_LIMIT))
}

fn scroll_summary(direction: Option<deckhand_core::protocol::Direction>, amount: u32, place: &str) -> String {
    let direction = direction.map_or("", |direction| direction.name());
    format!("scroll {direction} {amount}{place}")
}

fn drag_summary(from: Option<&TargetSpec>, to: Option<&TargetSpec>) -> String {
    let end = |target: Option<&TargetSpec>| target.map_or_else(|| "?".to_string(), label);
    format!("drag {} to {}", end(from), end(to))
}

fn set_summary(target: &TargetSpec, change: &SetChange) -> String {
    let target = label(target);
    match change {
        SetChange::Value(value) => format!("set {target} to {}", quoted(value, TEXT_LIMIT)),
        SetChange::Option(option) => format!("choose {} in {target}", quoted(option, TEXT_LIMIT)),
        SetChange::Checked(checked) => format!("{} {target}", if *checked { "check" } else { "uncheck" }),
        SetChange::Expanded(expanded) => format!("{} {target}", if *expanded { "expand" } else { "collapse" }),
        SetChange::Selected(selected) => format!("{} {target}", if *selected { "select" } else { "deselect" }),
    }
}

fn window_summary(action: WindowAction, target: Option<&TargetSpec>, app: Option<&str>) -> String {
    match (action, target) {
        (WindowAction::Launch, _) => format!("launch {}", clipped(app.unwrap_or_default().trim(), TARGET_LIMIT)),
        (_, Some(target)) => format!("{} {}", action.name(), label(target)),
        (_, None) => action.name().to_string(),
    }
}

pub(crate) fn file_count(count: usize) -> String {
    if count == 1 { "1 file".to_string() } else { format!("{count} files") }
}

fn wait_summary(args: &WaitArgs) -> String {
    let field = |value: &Option<String>| clipped(value.as_deref().unwrap_or_default().trim(), TARGET_LIMIT);
    match args.until() {
        Until::Time => format!("wait {} ms", args.ms.unwrap_or(0)),
        Until::Appears => format!("wait until {} appears", field(&args.target)),
        Until::Disappears => format!("wait until {} disappears", field(&args.target)),
        Until::Window => format!("wait for window {}", field(&args.window)),
        Until::WindowOpens => format!("wait for window {} to open", field(&args.window)),
        Until::WindowClosed => format!("wait for window {} to close", field(&args.window)),
        Until::Text => format!("wait for text {}", quoted(args.text.as_deref().unwrap_or_default(), TEXT_LIMIT)),
        Until::Idle => match args.ms {
            Some(ms) => format!("wait until idle for {ms} ms"),
            None => "wait until idle".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(json: &str) -> String {
        summarize(&serde_json::from_str(json).unwrap())
    }

    #[test]
    fn clicks_name_button_count_and_modifiers() {
        assert_eq!(summary(r#"{"do":"click","target":"e12"}"#), "click e12");
        assert_eq!(summary(r#"{"do":"click","target":"e12","count":2}"#), "double-click e12");
        assert_eq!(summary(r#"{"do":"click","target":"e12","count":3}"#), "triple-click e12");
        assert_eq!(summary(r#"{"do":"click","target":"e12","button":"right"}"#), "right-click e12");
        assert_eq!(summary(r#"{"do":"click","target":"e12","button":"middle","count":2}"#), "middle-double-click e12");
        assert_eq!(summary(r#"{"do":"click","target":"e12","modifiers":"ctrl+shift"}"#), "click e12 with ctrl+shift");
        assert_eq!(summary(r#"{"do":"click","target":"e12","modifiers":" "}"#), "click e12");
    }

    #[test]
    fn point_targets_are_shown_as_coordinates() {
        assert_eq!(
            summary(r#"{"do":"mouse","action":"move","target":{"x":640,"y":360,"img":"i3"}}"#),
            "mouse move at 640,360@i3"
        );
    }

    #[test]
    fn typed_text_is_clipped_to_24_characters() {
        assert_eq!(
            summary(r#"{"do":"type","target":"e3","text":"hello world, this is a long sentence"}"#),
            "type \"hello world, this is a l…\" into e3"
        );
        assert_eq!(summary(r#"{"do":"type","text":"hi\nthere","enter":true}"#), "type \"hi\\nthere\" + enter");
    }

    #[test]
    fn set_steps_say_what_they_change() {
        assert_eq!(
            summary(r#"{"do":"set","target":"e3","value":"C:\\temp\\a.txt"}"#),
            "set e3 to \"C:\\\\temp\\\\a.txt\""
        );
        assert_eq!(summary(r#"{"do":"set","target":"e7","option":"Blue"}"#), "choose \"Blue\" in e7");
        assert_eq!(summary(r#"{"do":"set","target":"e5","checked":true}"#), "check e5");
        assert_eq!(summary(r#"{"do":"set","target":"e5","checked":false}"#), "uncheck e5");
        assert_eq!(summary(r#"{"do":"set","target":"e5","expanded":false}"#), "collapse e5");
        assert_eq!(summary(r#"{"do":"set","target":"e5","selected":true}"#), "select e5");
        assert_eq!(summary(r#"{"do":"set","target":"e5"}"#), "set e5");
    }

    #[test]
    fn keys_and_mouse_actions() {
        assert_eq!(summary(r#"{"do":"key","keys":"ctrl+s"}"#), "key ctrl+s");
        assert_eq!(summary(r#"{"do":"key","keys":"down","target":"e9","repeat":5}"#), "key down in e9 ×5");
        assert_eq!(summary(r#"{"do":"key","keys":"shift","action":"down"}"#), "key down shift");
        assert_eq!(summary(r#"{"do":"key","keys":"shift","action":"up"}"#), "key up shift");
        assert_eq!(
            summary(r#"{"do":"mouse","action":"scroll","target":"e5","direction":"down"}"#),
            "scroll down 3 at e5"
        );
        assert_eq!(summary(r#"{"do":"mouse","action":"scroll","direction":"up","amount":1}"#), "scroll up 1");
        assert_eq!(summary(r#"{"do":"mouse","action":"drag","target":"e1","to":"e2"}"#), "drag e1 to e2");
        assert_eq!(
            summary(r#"{"do":"mouse","action":"down","target":"e4","button":"right"}"#),
            "right mouse down at e4"
        );
        assert_eq!(summary(r#"{"do":"mouse","action":"up"}"#), "mouse up");
    }

    #[test]
    fn window_pick_clipboard_notify_and_focus() {
        assert_eq!(summary(r#"{"do":"window","action":"launch","app":"Notepad"}"#), "launch Notepad");
        assert_eq!(summary(r#"{"do":"window","action":"close","target":"w42"}"#), "close w42");
        assert_eq!(summary(r#"{"do":"pick","paths":["C:\\a.txt","C:\\b.txt"]}"#), "pick 2 files");
        assert_eq!(summary(r#"{"do":"clipboard","text":"copied"}"#), "set the clipboard to \"copied\"");
        assert_eq!(summary(r#"{"do":"notify","title":"Export done","message":"m"}"#), "notify \"Export done\"");
        assert_eq!(summary(r#"{"do":"focus","target":"w42"}"#), "focus w42");
    }

    #[test]
    fn long_selectors_are_clipped() {
        let long = format!(r#"{{"do":"focus","target":"name:{}"}}"#, "x".repeat(100));
        let text = summary(&long);
        assert_eq!(text.chars().count(), "focus ".len() + 61);
        assert!(text.ends_with('…'));
    }

    #[test]
    fn waits_name_their_condition() {
        assert_eq!(summary(r#"{"do":"wait","ms":500}"#), "wait 500 ms");
        assert_eq!(summary(r#"{"do":"wait","until":"appears","target":"name:Done"}"#), "wait until name:Done appears");
        assert_eq!(summary(r#"{"do":"wait","until":"disappears","target":"e4"}"#), "wait until e4 disappears");
        assert_eq!(summary(r#"{"do":"wait","until":"window","window":"Save As"}"#), "wait for window Save As");
        assert_eq!(
            summary(r#"{"do":"wait","until":"window_closed","window":"Save As"}"#),
            "wait for window Save As to close"
        );
        assert_eq!(summary(r#"{"do":"wait","until":"text","text":"Saved"}"#), "wait for text \"Saved\"");
        assert_eq!(summary(r#"{"do":"wait","until":"idle"}"#), "wait until idle");
        assert_eq!(summary(r#"{"do":"wait","until":"idle","ms":300}"#), "wait until idle for 300 ms");
    }
}
