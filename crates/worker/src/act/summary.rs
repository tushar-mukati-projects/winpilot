use deckhand_core::{
    TargetSpec,
    protocol::{Condition, Direction, MouseButton, Step, Until},
};

use super::clip::{clipped, quoted};

const TEXT_LIMIT: usize = 24;
const TARGET_LIMIT: usize = 60;

pub fn summarize(step: &Step) -> String {
    match step {
        Step::Click { target, button, count, modifiers, .. } => {
            click_summary(target, *button, *count, modifiers.as_deref())
        }
        Step::Hover { target, .. } => format!("hover {}", label(target)),
        Step::Move { to, .. } => format!("move to {}", label(to)),
        Step::MouseDown { target, button, .. } => format!("{} {}", press_verb(*button, "down"), label(target)),
        Step::MouseUp { target: Some(target), button, .. } => {
            format!("{} at {}", press_verb(*button, "up"), label(target))
        }
        Step::MouseUp { target: None, button, .. } => press_verb(*button, "up"),
        Step::Type { target, text, enter, .. } => type_summary(target.as_ref(), text, *enter),
        Step::SetValue { target, value } => format!("set {} to {}", label(target), quoted(value, TEXT_LIMIT)),
        Step::Key { keys, target, repeat } => key_summary(keys, target.as_ref(), *repeat),
        Step::Scroll { target, direction, amount, .. } => scroll_summary(target.as_ref(), *direction, *amount),
        Step::Drag { from, to, .. } => format!("drag {} to {}", label(from), label(to)),
        Step::Select { target, option: Some(option) } => {
            format!("select {} in {}", quoted(option, TEXT_LIMIT), label(target))
        }
        Step::Select { target, option: None } => format!("select {}", label(target)),
        Step::Toggle { target, on } => toggle_summary(target, *on),
        Step::Expand { target } => format!("expand {}", label(target)),
        Step::Collapse { target } => format!("collapse {}", label(target)),
        Step::Focus { target } => format!("focus {}", label(target)),
        Step::WaitFor(condition) => wait_for_summary(condition),
        Step::Wait { ms } => format!("wait {ms} ms"),
    }
}

fn label(target: &TargetSpec) -> String {
    clipped(&target.to_string(), TARGET_LIMIT)
}

fn press_verb(button: MouseButton, direction: &str) -> String {
    match button {
        MouseButton::Left => format!("mouse {direction}"),
        MouseButton::Right => format!("right mouse {direction}"),
        MouseButton::Middle => format!("middle mouse {direction}"),
    }
}

fn click_summary(target: &TargetSpec, button: MouseButton, count: u8, modifiers: Option<&str>) -> String {
    let held = modifiers.map(str::trim).filter(|modifiers| !modifiers.is_empty());
    let held = held.map(|modifiers| format!(" with {modifiers}")).unwrap_or_default();
    format!("{} {}{held}", click_verb(button, count), label(target))
}

fn click_verb(button: MouseButton, count: u8) -> String {
    let clicks = match count {
        1 => "click".to_string(),
        2 => "double-click".to_string(),
        3 => "triple-click".to_string(),
        other => format!("click ×{other}"),
    };
    match button {
        MouseButton::Left => clicks,
        MouseButton::Right => format!("right-{clicks}"),
        MouseButton::Middle => format!("middle-{clicks}"),
    }
}

fn type_summary(target: Option<&TargetSpec>, text: &str, enter: bool) -> String {
    let into = target.map(|target| format!(" into {}", label(target))).unwrap_or_default();
    let enter = if enter { " + enter" } else { "" };
    format!("type {}{into}{enter}", quoted(text, TEXT_LIMIT))
}

fn key_summary(keys: &str, target: Option<&TargetSpec>, repeat: u32) -> String {
    let inside = target.map(|target| format!(" in {}", label(target))).unwrap_or_default();
    let times = if repeat > 1 { format!(" ×{repeat}") } else { String::new() };
    format!("key {}{inside}{times}", clipped(keys.trim(), TARGET_LIMIT))
}

fn scroll_summary(target: Option<&TargetSpec>, direction: Direction, amount: u32) -> String {
    let summary = format!("scroll {} {amount}", direction_word(direction));
    match target {
        Some(target) => format!("{summary} at {}", label(target)),
        None => summary,
    }
}

fn toggle_summary(target: &TargetSpec, on: Option<bool>) -> String {
    match on {
        Some(true) => format!("toggle {} on", label(target)),
        Some(false) => format!("toggle {} off", label(target)),
        None => format!("toggle {}", label(target)),
    }
}

fn wait_for_summary(condition: &Condition) -> String {
    let field = |value: &Option<String>| clipped(value.as_deref().unwrap_or_default().trim(), TARGET_LIMIT);
    match condition.until {
        Until::Appears => format!("wait_for appears {}", field(&condition.target)),
        Until::Disappears => format!("wait_for disappears {}", field(&condition.target)),
        Until::Window => format!("wait_for window {}", field(&condition.window)),
        Until::WindowClosed => format!("wait_for window_closed {}", field(&condition.window)),
        Until::Text => format!("wait_for text {}", quoted(condition.text.as_deref().unwrap_or_default(), TEXT_LIMIT)),
        Until::Idle => match condition.ms {
            Some(ms) => format!("wait_for idle {ms} ms"),
            None => "wait_for idle".to_string(),
        },
    }
}

const fn direction_word(direction: Direction) -> &'static str {
    match direction {
        Direction::Up => "up",
        Direction::Down => "down",
        Direction::Left => "left",
        Direction::Right => "right",
    }
}

#[cfg(test)]
mod tests {
    use deckhand_core::{
        protocol::{ClickHow, TypeHow},
        target::PointSpec,
    };

    use super::*;

    fn text(target: &str) -> TargetSpec {
        TargetSpec::Text(target.to_string())
    }

    fn click(button: MouseButton, count: u8, modifiers: Option<&str>) -> Step {
        Step::Click {
            target: text("e12"),
            button,
            count,
            modifiers: modifiers.map(str::to_string),
            how: ClickHow::Auto,
            glide_ms: None,
        }
    }

    fn condition(until: Until) -> Condition {
        Condition { until, target: None, text: None, window: None, ms: None, timeout_ms: None }
    }

    #[test]
    fn clicks_name_button_count_and_modifiers() {
        assert_eq!(summarize(&click(MouseButton::Left, 1, None)), "click e12");
        assert_eq!(summarize(&click(MouseButton::Left, 2, None)), "double-click e12");
        assert_eq!(summarize(&click(MouseButton::Left, 3, None)), "triple-click e12");
        assert_eq!(summarize(&click(MouseButton::Right, 1, None)), "right-click e12");
        assert_eq!(summarize(&click(MouseButton::Middle, 2, None)), "middle-double-click e12");
        assert_eq!(summarize(&click(MouseButton::Left, 1, Some("ctrl+shift"))), "click e12 with ctrl+shift");
        assert_eq!(summarize(&click(MouseButton::Left, 1, Some(" "))), "click e12");
    }

    #[test]
    fn point_targets_are_shown_as_coordinates() {
        let point = TargetSpec::Point(PointSpec { x: 640, y: 360, img: Some("i3".to_string()) });
        assert_eq!(summarize(&Step::Hover { target: point, glide_ms: None }), "hover 640,360@i3");
    }

    #[test]
    fn typed_text_is_clipped_to_24_characters() {
        let step = Step::Type {
            target: Some(text("e3")),
            text: "hello world, this is a long sentence".to_string(),
            enter: false,
            how: TypeHow::Auto,
        };
        assert_eq!(summarize(&step), "type \"hello world, this is a l…\" into e3");
        let short = Step::Type { target: None, text: "hi\nthere".to_string(), enter: true, how: TypeHow::Keys };
        assert_eq!(summarize(&short), "type \"hi\\nthere\" + enter");
    }

    #[test]
    fn set_value_select_and_toggle() {
        let set = Step::SetValue { target: text("e3"), value: "C:\\temp\\a.txt".to_string() };
        assert_eq!(summarize(&set), "set e3 to \"C:\\\\temp\\\\a.txt\"");
        let pick = Step::Select { target: text("e7"), option: Some("Blue".to_string()) };
        assert_eq!(summarize(&pick), "select \"Blue\" in e7");
        assert_eq!(summarize(&Step::Select { target: text("e7"), option: None }), "select e7");
        assert_eq!(summarize(&Step::Toggle { target: text("e5"), on: Some(true) }), "toggle e5 on");
        assert_eq!(summarize(&Step::Toggle { target: text("e5"), on: Some(false) }), "toggle e5 off");
        assert_eq!(summarize(&Step::Toggle { target: text("e5"), on: None }), "toggle e5");
    }

    #[test]
    fn keys_scroll_and_drag() {
        assert_eq!(summarize(&Step::Key { keys: "ctrl+s".to_string(), target: None, repeat: 1 }), "key ctrl+s");
        let repeated = Step::Key { keys: "down".to_string(), target: Some(text("e9")), repeat: 5 };
        assert_eq!(summarize(&repeated), "key down in e9 ×5");
        let scroll = Step::Scroll { target: Some(text("e5")), direction: Direction::Down, amount: 3, glide_ms: None };
        assert_eq!(summarize(&scroll), "scroll down 3 at e5");
        assert_eq!(
            summarize(&Step::Scroll { target: None, direction: Direction::Up, amount: 1, glide_ms: None }),
            "scroll up 1"
        );
        assert_eq!(summarize(&Step::Drag { from: text("e1"), to: text("e2"), glide_ms: None }), "drag e1 to e2");
    }

    #[test]
    fn simple_target_steps() {
        assert_eq!(summarize(&Step::Expand { target: text("e3") }), "expand e3");
        assert_eq!(summarize(&Step::Collapse { target: text("e3") }), "collapse e3");
        assert_eq!(summarize(&Step::Focus { target: text("w42") }), "focus w42");
        assert_eq!(summarize(&Step::Wait { ms: 500 }), "wait 500 ms");
    }

    #[test]
    fn long_selectors_are_clipped() {
        let long = format!("name:\"{}\"", "x".repeat(100));
        let summary = summarize(&Step::Focus { target: text(&long) });
        assert_eq!(summary.chars().count(), "focus ".len() + 61);
        assert!(summary.ends_with('…'));
    }

    #[test]
    fn wait_for_names_its_condition() {
        let appears = Condition { target: Some("name:Done".to_string()), ..condition(Until::Appears) };
        assert_eq!(summarize(&Step::WaitFor(appears)), "wait_for appears name:Done");
        let gone = Condition { target: Some("e4".to_string()), ..condition(Until::Disappears) };
        assert_eq!(summarize(&Step::WaitFor(gone)), "wait_for disappears e4");
        let window = Condition { window: Some("Save As".to_string()), ..condition(Until::Window) };
        assert_eq!(summarize(&Step::WaitFor(window)), "wait_for window Save As");
        let closed = Condition { window: Some("Save As".to_string()), ..condition(Until::WindowClosed) };
        assert_eq!(summarize(&Step::WaitFor(closed)), "wait_for window_closed Save As");
        let shown = Condition { text: Some("Saved".to_string()), ..condition(Until::Text) };
        assert_eq!(summarize(&Step::WaitFor(shown)), "wait_for text \"Saved\"");
        assert_eq!(summarize(&Step::WaitFor(condition(Until::Idle))), "wait_for idle");
        let idle = Condition { ms: Some(300), ..condition(Until::Idle) };
        assert_eq!(summarize(&Step::WaitFor(idle)), "wait_for idle 300 ms");
    }
}
