use deckhand_core::{
    Rect, TargetSpec,
    protocol::{
        ActArgs, Call, ClipboardArgs, Condition, FindArgs, MouseButton, ObserveArgs, PickFileArgs, ReadTextArgs,
        ScreenshotArgs, Step, WindowsArgs,
    },
};
use serde::Serialize;

const MAX_SUMMARY_CHARS: usize = 600;

pub fn describe(call: &Call, log_text: bool) -> String {
    let summary = match call {
        Call::Observe(args) => observe(args),
        Call::Find(args) => find(args),
        Call::Screenshot(args) => screenshot(args),
        Call::Act(args) => act(args, log_text),
        Call::WaitFor(condition) => wait_for(condition),
        Call::Windows(args) => windows(args),
        Call::PickFile(args) => pick_file(args),
        Call::ReadText(args) => read_text(args),
        Call::Clipboard(args) => clipboard(args, log_text),
        Call::Status => String::new(),
    };
    clip(summary)
}

fn observe(args: &ObserveArgs) -> String {
    fields(&[
        ("window", args.window.clone()),
        ("query", args.query.clone()),
        ("all", flag(args.all)),
        ("max", args.max.map(|max| max.to_string())),
    ])
}

fn find(args: &FindArgs) -> String {
    fields(&[
        ("query", Some(args.query.clone())),
        ("window", args.window.clone()),
        ("all_windows", flag(args.all_windows)),
    ])
}

fn screenshot(args: &ScreenshotArgs) -> String {
    fields(&[
        ("target", args.target.clone()),
        ("region", args.region.map(rect)),
        ("monitor", args.monitor.map(|monitor| monitor.to_string())),
        ("marks", flag(args.marks)),
    ])
}

fn act(args: &ActArgs, log_text: bool) -> String {
    let steps: Vec<String> = args.steps.iter().map(|step| step_summary(step, log_text)).collect();
    format!("{} return={}", steps.join("; "), wire_name(&args.after))
}

fn step_summary(step: &Step, log_text: bool) -> String {
    match step {
        Step::Click { target, button, count, .. } => click(target, *button, *count),
        Step::Hover { target } => format!("hover {target}"),
        Step::Type { target, text, enter, .. } => type_text(target.as_ref(), text, *enter, log_text),
        Step::SetValue { target, value } => format!("set_value {target} {}", typed(value, log_text)),
        Step::Key { keys, target, repeat } => key(keys, target.as_ref(), *repeat),
        Step::Scroll { target, direction, amount } => {
            format!("scroll {} {amount}{}", wire_name(direction), on(target.as_ref()))
        }
        Step::Drag { from, to } => format!("drag {from} -> {to}"),
        Step::Select { target, option } => select(target, option.as_deref()),
        Step::Toggle { target, on } => {
            format!("toggle {target}{}", on.map_or("", |state| if state { " on" } else { " off" }))
        }
        Step::Expand { target } => format!("expand {target}"),
        Step::Collapse { target } => format!("collapse {target}"),
        Step::Focus { target } => format!("focus {target}"),
        Step::WaitFor(condition) => format!("wait_for {}", wait_for(condition)),
        Step::Wait { ms } => format!("wait {ms}ms"),
    }
}

fn click(target: &TargetSpec, button: MouseButton, count: u8) -> String {
    let button = if button == MouseButton::Left { String::new() } else { format!(" {}", wire_name(&button)) };
    let count = if count > 1 { format!(" x{count}") } else { String::new() };
    format!("click {target}{button}{count}")
}

fn type_text(target: Option<&TargetSpec>, text: &str, enter: bool, log_text: bool) -> String {
    let enter = if enter { " +enter" } else { "" };
    let into = target.map(|target| format!(" into {target}")).unwrap_or_default();
    format!("type {}{into}{enter}", typed(text, log_text))
}

fn key(keys: &str, target: Option<&TargetSpec>, repeat: u32) -> String {
    let repeat = if repeat > 1 { format!(" x{repeat}") } else { String::new() };
    format!("key {keys}{repeat}{}", on(target))
}

fn select(target: &TargetSpec, option: Option<&str>) -> String {
    match option {
        Some(option) => format!("select {target} option={option:?}"),
        None => format!("select {target}"),
    }
}

fn on(target: Option<&TargetSpec>) -> String {
    target.map(|target| format!(" on {target}")).unwrap_or_default()
}

fn wait_for(condition: &Condition) -> String {
    let details = fields(&[
        ("target", condition.target.clone()),
        ("text", condition.text.clone()),
        ("window", condition.window.clone()),
        ("ms", condition.ms.map(|ms| ms.to_string())),
        ("timeout_ms", condition.timeout_ms.map(|ms| ms.to_string())),
    ]);
    joined(&wire_name(&condition.until), &details)
}

fn windows(args: &WindowsArgs) -> String {
    let details = fields(&[
        ("window", args.window.clone()),
        ("app", args.app.clone()),
        ("args", args.args.clone()),
        ("rect", args.rect.map(rect)),
    ]);
    joined(&wire_name(&args.action), &details)
}

fn pick_file(args: &PickFileArgs) -> String {
    fields(&[
        ("paths", Some(args.paths.join("|"))),
        ("open_with", args.open_with.as_ref().map(ToString::to_string)),
        ("overwrite", flag(args.overwrite)),
    ])
}

fn read_text(args: &ReadTextArgs) -> String {
    fields(&[
        ("target", args.target.clone()),
        ("region", args.region.map(rect)),
        ("method", Some(wire_name(&args.method))),
    ])
}

fn clipboard(args: &ClipboardArgs, log_text: bool) -> String {
    let text = args.text.as_deref().map(|text| typed(text, log_text));
    joined(&wire_name(&args.action), &text.unwrap_or_default())
}

fn rect(rect: Rect) -> String {
    format!("{},{},{},{}", rect.x, rect.y, rect.width, rect.height)
}

fn flag(set: bool) -> Option<String> {
    set.then(|| "true".to_string())
}

fn typed(text: &str, log_text: bool) -> String {
    if log_text { format!("{text:?}") } else { format!("<{} chars>", text.chars().count()) }
}

fn fields(pairs: &[(&str, Option<String>)]) -> String {
    pairs
        .iter()
        .filter_map(|(name, value)| value.as_ref().map(|value| format!("{name}={value}")))
        .collect::<Vec<_>>()
        .join(" ")
}

fn joined(head: &str, tail: &str) -> String {
    if tail.is_empty() { head.to_string() } else { format!("{head} {tail}") }
}

fn wire_name<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value).ok().and_then(|value| value.as_str().map(str::to_string)).unwrap_or_default()
}

fn clip(summary: String) -> String {
    if summary.chars().count() <= MAX_SUMMARY_CHARS {
        return summary;
    }
    let mut clipped: String = summary.chars().take(MAX_SUMMARY_CHARS).collect();
    clipped.push('…');
    clipped
}

#[cfg(test)]
mod tests {
    use deckhand_core::protocol::{AfterMode, ClickHow, ClipboardAction, Direction, TypeHow, Until};

    use super::*;

    fn text_target(text: &str) -> TargetSpec {
        TargetSpec::Text(text.to_string())
    }

    fn act_call(steps: Vec<Step>) -> Call {
        Call::Act(ActArgs { steps, after: AfterMode::Diff, stop_on_error: true })
    }

    fn type_step(text: &str) -> Step {
        Step::Type { target: Some(text_target("role:edit")), text: text.to_string(), enter: true, how: TypeHow::Auto }
    }

    #[test]
    fn typed_text_is_redacted_by_default() {
        let summary = describe(&act_call(vec![type_step("hunter2")]), false);
        assert_eq!(summary, "type <7 chars> into role:edit +enter return=diff");
        assert!(!summary.contains("hunter2"));
    }

    #[test]
    fn typed_text_is_logged_when_enabled() {
        let summary = describe(&act_call(vec![type_step("hunter2")]), true);
        assert!(summary.contains("\"hunter2\""), "{summary}");
    }

    #[test]
    fn set_value_and_clipboard_text_are_redacted() {
        let set = Step::SetValue { target: text_target("id:1001"), value: "C:\\secret.txt".to_string() };
        assert_eq!(describe(&act_call(vec![set]), false), "set_value id:1001 <13 chars> return=diff");
        let clipboard = Call::Clipboard(ClipboardArgs { action: ClipboardAction::Set, text: Some("pw".to_string()) });
        assert_eq!(describe(&clipboard, false), "set <2 chars>");
        assert_eq!(describe(&clipboard, true), "set \"pw\"");
    }

    #[test]
    fn act_lists_step_kinds_and_targets() {
        let steps = vec![
            Step::Click {
                target: text_target("e12"),
                button: MouseButton::Right,
                count: 2,
                modifiers: None,
                how: ClickHow::Auto,
            },
            Step::Key { keys: "ctrl+s".to_string(), target: None, repeat: 1 },
            Step::Scroll { target: Some(text_target("e3")), direction: Direction::Down, amount: 5 },
            Step::Wait { ms: 200 },
        ];
        assert_eq!(
            describe(&act_call(steps), false),
            "click e12 right x2; key ctrl+s; scroll down 5 on e3; wait 200ms return=diff"
        );
    }

    #[test]
    fn wait_for_condition_is_summarised() {
        let condition = Condition {
            until: Until::Appears,
            target: Some("name:Done".to_string()),
            text: None,
            window: None,
            ms: None,
            timeout_ms: Some(2000),
        };
        assert_eq!(describe(&Call::WaitFor(condition), false), "appears target=name:Done timeout_ms=2000");
    }

    #[test]
    fn status_has_an_empty_summary() {
        assert_eq!(describe(&Call::Status, false), "");
    }

    #[test]
    fn long_summaries_are_clipped() {
        let steps = vec![Step::Hover { target: text_target(&"x".repeat(2000)) }];
        let summary = describe(&act_call(steps), false);
        assert_eq!(summary.chars().count(), MAX_SUMMARY_CHARS + 1);
        assert!(summary.ends_with('…'));
    }
}
