use std::path::Path;

use deckhand_core::{
    Rect, TargetSpec,
    protocol::{ActArgs, Call, KeyAction, MouseAction, MouseButton, ObserveArgs, SetChange, Step, WaitArgs},
};
use serde::Serialize;

const MAX_SUMMARY_CHARS: usize = 600;

pub fn describe(call: &Call, log_text: bool) -> String {
    let summary = match call {
        Call::Observe(args) => observe(args),
        Call::Act(args) => act(args, log_text),
    };
    clip(summary)
}

fn observe(args: &ObserveArgs) -> String {
    let details = fields(&[
        ("window", args.window.clone()),
        ("target", args.target.clone()),
        ("query", args.query.clone()),
        ("all_windows", flag(args.all_windows)),
        ("all", flag(args.all)),
        ("max", args.max.map(|max| max.to_string())),
        ("region", args.region.map(rect)),
        ("monitor", args.monitor.map(|monitor| monitor.to_string())),
        ("ocr", args.ocr.as_ref().map(wire_name)),
        ("preset", args.preset.map(|preset| preset.name().to_string())),
        ("quality", args.quality.map(|quality| quality.to_string())),
        ("video_quality", args.video_quality.map(|quality| quality.name().to_string())),
        ("format", args.format.map(|format| format.name().to_string())),
        ("marks", flag(args.marks)),
        ("action", args.action.as_ref().map(wire_name)),
        ("fps", args.fps.map(|fps| fps.to_string())),
        ("name", args.path.as_deref().map(base_name)),
    ]);
    joined(&format!("mode={}", args.mode.name()), &details)
}

fn act(args: &ActArgs, log_text: bool) -> String {
    let steps: Vec<String> = args.steps.iter().map(|step| step_summary(step, log_text)).collect();
    let background = match args.background {
        Some(true) => " background",
        Some(false) | None => "",
    };
    format!("{} return={}{background}", steps.join("; "), wire_name(&args.after))
}

fn step_summary(step: &Step, log_text: bool) -> String {
    match step {
        Step::Click { target, button, count, .. } => click(target, *button, *count),
        Step::Type { target, text, enter, .. } => type_text(target.as_ref(), text, *enter, log_text),
        Step::Key { keys, target, repeat, action } => key(keys, target.as_ref(), *repeat, *action),
        Step::Mouse { action, target, to, button, direction, amount, glide_ms } => {
            let mut parts = vec![format!("mouse {}", action.name())];
            parts.extend(target.as_ref().map(ToString::to_string));
            parts.extend(to.as_ref().map(|to| format!("-> {to}")));
            parts.extend(button.filter(|button| *button != MouseButton::Left).map(|button| button.name().to_string()));
            parts.extend(
                direction.filter(|_| *action == MouseAction::Scroll).map(|direction| direction.name().to_string()),
            );
            parts.extend(amount.map(|amount| amount.to_string()));
            parts.extend(glide_ms.map(|ms| format!("glide={ms}ms")));
            parts.join(" ")
        }
        Step::Set { target, .. } => set(target, step.set_change().ok().as_ref(), log_text),
        Step::Focus { target } => format!("focus {target}"),
        Step::Window { action, target, app, args, rect: area } => {
            let details = fields(&[
                ("target", target.as_ref().map(ToString::to_string)),
                ("app", app.clone()),
                ("args", args.as_deref().map(|text| typed(text, log_text))),
                ("rect", area.map(rect)),
            ]);
            joined(&format!("window {}", action.name()), &details)
        }
        Step::Pick { paths, open_with, overwrite, .. } => pick(paths, open_with.as_ref(), *overwrite, log_text),
        Step::Wait(args) => format!("wait {}", wait(args, log_text)),
        Step::Clipboard { text } => format!("clipboard {}", typed(text, log_text)),
        Step::Notify { title, message } => {
            format!("notify title={} message={}", typed(title, false), typed(message, false))
        }
    }
}

fn click(target: &TargetSpec, button: MouseButton, count: u8) -> String {
    let button = if button == MouseButton::Left { String::new() } else { format!(" {}", button.name()) };
    let count = if count > 1 { format!(" x{count}") } else { String::new() };
    format!("click {target}{button}{count}")
}

fn type_text(target: Option<&TargetSpec>, text: &str, enter: bool, log_text: bool) -> String {
    let enter = if enter { " +enter" } else { "" };
    let into = target.map(|target| format!(" into {target}")).unwrap_or_default();
    format!("type {}{into}{enter}", typed(text, log_text))
}

fn key(keys: &str, target: Option<&TargetSpec>, repeat: u32, action: KeyAction) -> String {
    let action = if action == KeyAction::Press { String::new() } else { format!(" {}", action.name()) };
    let repeat = if repeat > 1 { format!(" x{repeat}") } else { String::new() };
    format!("key {keys}{action}{repeat}{}", on(target))
}

fn set(target: &TargetSpec, change: Option<&SetChange>, log_text: bool) -> String {
    let change = match change {
        Some(SetChange::Value(value)) => format!(" value={}", typed(value, log_text)),
        Some(SetChange::Option(option)) => format!(" option={option:?}"),
        Some(SetChange::Checked(checked)) => format!(" checked={checked}"),
        Some(SetChange::Expanded(expanded)) => format!(" expanded={expanded}"),
        Some(SetChange::Selected(selected)) => format!(" selected={selected}"),
        None => String::new(),
    };
    format!("set {target}{change}")
}

fn on(target: Option<&TargetSpec>) -> String {
    target.map(|target| format!(" on {target}")).unwrap_or_default()
}

fn wait(args: &WaitArgs, log_text: bool) -> String {
    let details = fields(&[
        ("target", args.target.clone()),
        ("text", args.text.as_deref().map(|text| typed(text, log_text))),
        ("window", args.window.clone()),
        ("ms", args.ms.map(|ms| ms.to_string())),
        ("timeout_ms", args.timeout_ms.map(|ms| ms.to_string())),
    ]);
    joined(args.until().name(), &details)
}

fn pick(paths: &[String], open_with: Option<&TargetSpec>, overwrite: bool, log_text: bool) -> String {
    let (label, paths_text) = if log_text { ("paths", paths.join("|")) } else { ("names", base_names(paths)) };
    let details = fields(&[
        ("files", Some(paths.len().to_string())),
        (label, Some(paths_text)),
        ("open_with", open_with.map(ToString::to_string)),
        ("overwrite", flag(Some(overwrite).filter(|overwrite| *overwrite))),
    ]);
    joined("pick", &details)
}

fn base_names(paths: &[String]) -> String {
    paths.iter().map(|path| base_name(path)).collect::<Vec<_>>().join("|")
}

fn base_name(path: &str) -> String {
    Path::new(path).file_name().map_or_else(|| typed(path, false), |name| name.to_string_lossy().into_owned())
}

fn rect(rect: Rect) -> String {
    format!("{},{},{},{}", rect.x, rect.y, rect.width, rect.height)
}

fn flag(set: Option<bool>) -> Option<String> {
    set.map(|set| set.to_string())
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
    use serde_json::{Value, json};

    use super::*;

    fn act_call(steps: &Value) -> Call {
        Call::Act(serde_json::from_value(json!({ "steps": steps })).unwrap())
    }

    fn observe_call(args: Value) -> Call {
        Call::Observe(serde_json::from_value(args).unwrap())
    }

    #[test]
    fn typed_text_is_redacted_by_default() {
        let call = act_call(&json!([{"do": "type", "target": "role:edit", "text": "hunter2", "enter": true}]));
        let summary = describe(&call, false);
        assert_eq!(summary, "type <7 chars> into role:edit +enter return=diff");
        assert!(!summary.contains("hunter2"));
        assert!(describe(&call, true).contains("\"hunter2\""));
    }

    #[test]
    fn set_values_and_clipboard_text_are_redacted() {
        let set = act_call(&json!([{"do": "set", "target": "id:1001", "value": "C:\\secret.txt"}]));
        assert_eq!(describe(&set, false), "set id:1001 value=<13 chars> return=diff");
        let clipboard = act_call(&json!([{"do": "clipboard", "text": "pw"}]));
        assert_eq!(describe(&clipboard, false), "clipboard <2 chars> return=diff");
        assert_eq!(describe(&clipboard, true), "clipboard \"pw\" return=diff");
        let checked = act_call(&json!([{"do": "set", "target": "e5", "checked": true}]));
        assert_eq!(describe(&checked, false), "set e5 checked=true return=diff");
    }

    #[test]
    fn launch_arguments_are_redacted() {
        let call = act_call(
            &json!([{"do": "window", "action": "launch", "app": "notepad", "args": "C:\\Users\\me\\tax-2025.pdf"}]),
        );
        assert_eq!(describe(&call, false), "window launch app=notepad args=<24 chars> return=diff");
        assert_eq!(
            describe(&call, true),
            "window launch app=notepad args=\"C:\\\\Users\\\\me\\\\tax-2025.pdf\" return=diff"
        );
    }

    #[test]
    fn picked_paths_keep_only_the_count_and_file_names() {
        let call = act_call(&json!([{
            "do": "pick",
            "paths": ["C:\\Users\\me\\Private\\tax-2025.pdf", "C:\\Users\\me\\Private\\id.png"],
            "open_with": "name:Upload"
        }]));
        let summary = describe(&call, false);
        assert_eq!(summary, "pick files=2 names=tax-2025.pdf|id.png open_with=name:Upload return=diff");
        assert!(!summary.contains("Private"));
        assert!(describe(&call, true).contains("paths=C:\\Users\\me\\Private\\tax-2025.pdf|"));
        let root = act_call(&json!([{"do": "pick", "paths": ["C:\\"]}]));
        assert_eq!(describe(&root, false), "pick files=1 names=<3 chars> return=diff");
    }

    #[test]
    fn waited_for_text_is_redacted() {
        let call =
            act_call(&json!([{"do": "wait", "until": "text", "text": "Payment 5000 received", "window": "Bank"}]));
        assert_eq!(describe(&call, false), "wait text text=<21 chars> window=Bank return=diff");
        assert!(describe(&call, true).contains("text=\"Payment 5000 received\""));
        let pause = act_call(&json!([{"do": "wait", "ms": 200}]));
        assert_eq!(describe(&pause, false), "wait time ms=200 return=diff");
    }

    #[test]
    fn notifications_log_only_their_lengths() {
        let call = act_call(&json!([{"do": "notify", "title": "Payroll", "message": "Salary file sent"}]));
        assert_eq!(describe(&call, false), "notify title=<7 chars> message=<16 chars> return=diff");
        assert_eq!(describe(&call, true), "notify title=<7 chars> message=<16 chars> return=diff");
    }

    #[test]
    fn act_lists_step_kinds_targets_and_background() {
        let call = Call::Act(
            serde_json::from_value(json!({
                "steps": [
                    {"do": "click", "target": "e12", "button": "right", "count": 2},
                    {"do": "key", "keys": "shift", "action": "down"},
                    {"do": "mouse", "action": "scroll", "target": "e3", "direction": "down", "amount": 5},
                    {"do": "mouse", "action": "drag", "target": "e1", "to": "e2"},
                    {"do": "focus", "target": "w7"},
                    {"do": "window", "action": "close", "target": "w7"}
                ],
                "return": "none",
                "background": true
            }))
            .unwrap(),
        );
        assert_eq!(
            describe(&call, false),
            "click e12 right x2; key shift down; mouse scroll e3 down 5; mouse drag e1 -> e2; focus w7; window close target=w7 return=none background"
        );
    }

    #[test]
    fn observe_names_the_mode_and_keeps_selectors() {
        let search = observe_call(json!({"query": "role:button name:Save", "all_windows": true}));
        assert_eq!(describe(&search, false), "mode=elements query=role:button name:Save all_windows=true");
        assert_eq!(describe(&observe_call(json!({"mode": "status"})), false), "mode=status");
        let shot = observe_call(json!({"mode": "screenshot", "monitor": 1, "preset": "max", "marks": true}));
        assert_eq!(describe(&shot, false), "mode=screenshot monitor=1 preset=max marks=true");
        let text = observe_call(json!({"mode": "text", "target": "e4", "ocr": "never"}));
        assert_eq!(describe(&text, false), "mode=text target=e4 ocr=never");
    }

    #[test]
    fn recordings_keep_only_the_file_name() {
        let start = observe_call(json!({
            "mode": "record", "action": "start", "monitor": 1, "format": "gif",
            "path": "C:\\Users\\me\\Private\\demo.gif"
        }));
        let summary = describe(&start, true);
        assert_eq!(summary, "mode=record monitor=1 format=gif action=start name=demo.gif");
        assert!(!summary.contains("Private"));
        assert_eq!(
            describe(&observe_call(json!({"mode": "record", "action": "stop"})), false),
            "mode=record action=stop"
        );
    }

    #[test]
    fn long_summaries_are_clipped() {
        let call = act_call(&json!([{"do": "focus", "target": "x".repeat(2000)}]));
        let summary = describe(&call, false);
        assert_eq!(summary.chars().count(), MAX_SUMMARY_CHARS + 1);
        assert!(summary.ends_with('…'));
    }
}
