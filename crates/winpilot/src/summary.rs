use std::path::Path;

use serde::Serialize;
use winpilot_core::{
    Rect, TargetSpec,
    config::GuardConfig,
    protocol::{ActArgs, Call, Direction, KeyAction, MouseAction, MouseButton, ObserveArgs, SetChange, Step, WaitArgs},
    text_limit::ellipsized,
};

const MAX_SUMMARY_CHARS: usize = 600;
const DENIED: &str = "<denied window>";

#[derive(Debug, Clone, Copy)]
pub struct Redaction<'a> {
    pub log_text: bool,
    pub guard: &'a GuardConfig,
}

struct MouseStep<'a> {
    action: MouseAction,
    target: Option<&'a TargetSpec>,
    to: Option<&'a TargetSpec>,
    button: Option<MouseButton>,
    direction: Option<Direction>,
    amount: Option<u32>,
    glide_ms: Option<u64>,
}

pub fn describe(call: &Call, redaction: Redaction<'_>) -> String {
    let summary = match call {
        Call::Observe(args) => observe(args, redaction),
        Call::Act(args) => act(args, redaction),
    };
    ellipsized(&summary, MAX_SUMMARY_CHARS)
}

fn observe(args: &ObserveArgs, redaction: Redaction<'_>) -> String {
    let details = fields(&[
        ("window", args.window.as_deref().map(|window| named(window, redaction))),
        ("target", args.target.as_deref().map(|target| named(target, redaction))),
        ("query", args.query.as_deref().map(|query| named(query, redaction))),
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
        ("audio", args.audio.map(|audio| audio.name().to_string())),
        ("name", args.path.as_deref().map(base_name)),
        ("language", args.language.clone()),
        ("translate", flag(args.translate)),
    ]);
    joined(&format!("mode={}", args.mode.name()), &details)
}

fn act(args: &ActArgs, redaction: Redaction<'_>) -> String {
    let steps: Vec<String> = args.steps.iter().map(|step| step_summary(step, redaction)).collect();
    let background = match args.background {
        Some(true) => " background",
        Some(false) | None => "",
    };
    format!("{} return={}{background}", steps.join("; "), wire_name(&args.after))
}

fn step_summary(step: &Step, redaction: Redaction<'_>) -> String {
    let log_text = redaction.log_text;
    match step {
        Step::Click { target, button, count, .. } => click(&spec(target, redaction), *button, *count),
        Step::Type { target, text, enter, .. } => type_text(target.as_ref(), text, *enter, redaction),
        Step::Key { keys, target, repeat, action } => key(keys, target.as_ref(), *repeat, *action, redaction),
        Step::Mouse { action, target, to, button, direction, amount, glide_ms } => {
            let mouse = MouseStep {
                action: *action,
                target: target.as_ref(),
                to: to.as_ref(),
                button: *button,
                direction: *direction,
                amount: *amount,
                glide_ms: *glide_ms,
            };
            mouse_summary(&mouse, redaction)
        }
        Step::Set { target, .. } => set(target, step.set_change().ok().as_ref(), redaction),
        Step::Focus { target } => format!("focus {}", spec(target, redaction)),
        Step::Window { action, target, app, args, rect: area } => {
            let details = fields(&[
                ("target", target.as_ref().map(|target| spec(target, redaction))),
                ("app", app.as_deref().map(|app| launched(app, log_text))),
                ("args", args.as_deref().map(|text| typed(text, log_text))),
                ("rect", area.map(rect)),
            ]);
            joined(&format!("window {}", action.name()), &details)
        }
        Step::Pick { paths, open_with, overwrite, .. } => pick(paths, open_with.as_ref(), *overwrite, redaction),
        Step::Wait(args) => format!("wait {}", wait(args, redaction)),
        Step::Clipboard { text } => format!("clipboard {}", typed(text, log_text)),
        Step::Notify { title, message } => {
            format!("notify title={} message={}", typed(title, false), typed(message, false))
        }
    }
}

fn mouse_summary(mouse: &MouseStep<'_>, redaction: Redaction<'_>) -> String {
    let mut parts = vec![format!("mouse {}", mouse.action.name())];
    parts.extend(mouse.target.map(|target| spec(target, redaction)));
    parts.extend(mouse.to.map(|to| format!("-> {}", spec(to, redaction))));
    parts.extend(mouse.button.filter(|button| *button != MouseButton::Left).map(|button| button.name().to_string()));
    let scrolling = mouse.action == MouseAction::Scroll;
    parts.extend(mouse.direction.filter(|_| scrolling).map(|direction| direction.name().to_string()));
    parts.extend(mouse.amount.map(|amount| amount.to_string()));
    parts.extend(mouse.glide_ms.map(|ms| format!("glide={ms}ms")));
    parts.join(" ")
}

fn click(target: &str, button: MouseButton, count: u8) -> String {
    let button = if button == MouseButton::Left { String::new() } else { format!(" {}", button.name()) };
    let count = if count > 1 { format!(" x{count}") } else { String::new() };
    format!("click {target}{button}{count}")
}

fn type_text(target: Option<&TargetSpec>, text: &str, enter: bool, redaction: Redaction<'_>) -> String {
    let enter = if enter { " +enter" } else { "" };
    let into = target.map(|target| format!(" into {}", spec(target, redaction))).unwrap_or_default();
    format!("type {}{into}{enter}", typed(text, redaction.log_text))
}

fn key(keys: &str, target: Option<&TargetSpec>, repeat: u32, action: KeyAction, redaction: Redaction<'_>) -> String {
    let action = if action == KeyAction::Press { String::new() } else { format!(" {}", action.name()) };
    let repeat = if repeat > 1 { format!(" x{repeat}") } else { String::new() };
    format!("key {}{action}{repeat}{}", typed(keys, redaction.log_text), on(target, redaction))
}

fn set(target: &TargetSpec, change: Option<&SetChange>, redaction: Redaction<'_>) -> String {
    let log_text = redaction.log_text;
    let change = match change {
        Some(SetChange::Value(value)) => format!(" value={}", typed(value, log_text)),
        Some(SetChange::Option(option)) => format!(" option={}", typed(option, log_text)),
        Some(SetChange::Checked(checked)) => format!(" checked={checked}"),
        Some(SetChange::Expanded(expanded)) => format!(" expanded={expanded}"),
        Some(SetChange::Selected(selected)) => format!(" selected={selected}"),
        None => String::new(),
    };
    format!("set {}{change}", spec(target, redaction))
}

fn on(target: Option<&TargetSpec>, redaction: Redaction<'_>) -> String {
    target.map(|target| format!(" on {}", spec(target, redaction))).unwrap_or_default()
}

fn wait(args: &WaitArgs, redaction: Redaction<'_>) -> String {
    let details = fields(&[
        ("target", args.target.as_deref().map(|target| named(target, redaction))),
        ("text", args.text.as_deref().map(|text| typed(text, redaction.log_text))),
        ("window", args.window.as_deref().map(|window| named(window, redaction))),
        ("ms", args.ms.map(|ms| ms.to_string())),
        ("timeout_ms", args.timeout_ms.map(|ms| ms.to_string())),
    ]);
    joined(args.until().name(), &details)
}

fn pick(paths: &[String], open_with: Option<&TargetSpec>, overwrite: bool, redaction: Redaction<'_>) -> String {
    let (label, paths_text) =
        if redaction.log_text { ("paths", paths.join("|")) } else { ("names", base_names(paths)) };
    let details = fields(&[
        ("files", Some(paths.len().to_string())),
        (label, Some(paths_text)),
        ("open_with", open_with.map(|target| spec(target, redaction))),
        ("overwrite", flag(Some(overwrite).filter(|overwrite| *overwrite))),
    ]);
    joined("pick", &details)
}

fn spec(target: &TargetSpec, redaction: Redaction<'_>) -> String {
    named(&target.to_string(), redaction)
}

fn named(text: &str, redaction: Redaction<'_>) -> String {
    if redaction.guard.mentions_denied(text) { DENIED.to_string() } else { text.to_string() }
}

fn launched(app: &str, log_text: bool) -> String {
    if log_text { app.to_string() } else { base_name(app) }
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

    fn quiet(call: &Call) -> String {
        describe(call, Redaction { log_text: false, guard: &GuardConfig::default() })
    }

    fn verbose(call: &Call) -> String {
        describe(call, Redaction { log_text: true, guard: &GuardConfig::default() })
    }

    #[test]
    fn typed_text_is_redacted_by_default() {
        let call = act_call(&json!([{"do": "type", "target": "role:edit", "text": "hunter2", "enter": true}]));
        let summary = quiet(&call);
        assert_eq!(summary, "type <7 chars> into role:edit +enter return=diff");
        assert!(!summary.contains("hunter2"));
        assert!(verbose(&call).contains("\"hunter2\""));
    }

    #[test]
    fn set_values_and_clipboard_text_are_redacted() {
        let set = act_call(&json!([{"do": "set", "target": "id:1001", "value": "C:\\secret.txt"}]));
        assert_eq!(quiet(&set), "set id:1001 value=<13 chars> return=diff");
        let clipboard = act_call(&json!([{"do": "clipboard", "text": "pw"}]));
        assert_eq!(quiet(&clipboard), "clipboard <2 chars> return=diff");
        assert_eq!(verbose(&clipboard), "clipboard \"pw\" return=diff");
        let checked = act_call(&json!([{"do": "set", "target": "e5", "checked": true}]));
        assert_eq!(quiet(&checked), "set e5 checked=true return=diff");
    }

    #[test]
    fn keys_options_and_launch_paths_are_redacted() {
        let keys = act_call(&json!([{"do": "key", "keys": "h e l l o"}]));
        assert_eq!(quiet(&keys), "key <9 chars> return=diff");
        assert_eq!(verbose(&keys), "key \"h e l l o\" return=diff");
        let option = act_call(&json!([{"do": "set", "target": "e2", "option": "Dr. Ada Lovelace"}]));
        assert_eq!(quiet(&option), "set e2 option=<16 chars> return=diff");
        let launch = act_call(&json!([{"do": "window", "action": "launch", "app": "C:\\Users\\me\\Tools\\tool.exe"}]));
        assert_eq!(quiet(&launch), "window launch app=tool.exe return=diff");
        assert!(verbose(&launch).contains("app=C:\\Users\\me\\Tools\\tool.exe"));
    }

    #[test]
    fn targets_that_name_a_denied_window_are_masked() {
        let mut guard = GuardConfig::default();
        guard.deny.push(winpilot_core::config::DenyRule { process: None, title: Some("Bank".to_string()) });
        let quiet_with = |call: &Call| describe(call, Redaction { log_text: true, guard: &guard });
        let click = act_call(&json!([{"do": "click", "target": "window:\"My Bank\" >> name:Pay"}]));
        assert_eq!(quiet_with(&click), "click <denied window> return=diff");
        let read = observe_call(json!({"mode": "text", "window": "process:keepass"}));
        assert_eq!(quiet_with(&read), "mode=text window=<denied window>");
        let wait = act_call(&json!([{"do": "wait", "until": "window", "window": "Online Banking"}]));
        assert_eq!(quiet_with(&wait), "wait window window=<denied window> return=diff");
        let fine = act_call(&json!([{"do": "focus", "target": "window:Notepad"}]));
        assert_eq!(quiet_with(&fine), "focus window:Notepad return=diff");
    }

    #[test]
    fn launch_arguments_are_redacted() {
        let call = act_call(
            &json!([{"do": "window", "action": "launch", "app": "notepad", "args": "C:\\Users\\me\\tax-2025.pdf"}]),
        );
        assert_eq!(quiet(&call), "window launch app=notepad args=<24 chars> return=diff");
        assert_eq!(verbose(&call), "window launch app=notepad args=\"C:\\\\Users\\\\me\\\\tax-2025.pdf\" return=diff");
    }

    #[test]
    fn picked_paths_keep_only_the_count_and_file_names() {
        let call = act_call(&json!([{
            "do": "pick",
            "paths": ["C:\\Users\\me\\Private\\tax-2025.pdf", "C:\\Users\\me\\Private\\id.png"],
            "open_with": "name:Upload"
        }]));
        let summary = quiet(&call);
        assert_eq!(summary, "pick files=2 names=tax-2025.pdf|id.png open_with=name:Upload return=diff");
        assert!(!summary.contains("Private"));
        assert!(verbose(&call).contains("paths=C:\\Users\\me\\Private\\tax-2025.pdf|"));
        let root = act_call(&json!([{"do": "pick", "paths": ["C:\\"]}]));
        assert_eq!(quiet(&root), "pick files=1 names=<3 chars> return=diff");
    }

    #[test]
    fn waited_for_text_is_redacted() {
        let call =
            act_call(&json!([{"do": "wait", "until": "text", "text": "Payment 5000 received", "window": "Bank"}]));
        assert_eq!(quiet(&call), "wait text text=<21 chars> window=Bank return=diff");
        assert!(verbose(&call).contains("text=\"Payment 5000 received\""));
        let pause = act_call(&json!([{"do": "wait", "ms": 200}]));
        assert_eq!(quiet(&pause), "wait time ms=200 return=diff");
    }

    #[test]
    fn notifications_log_only_their_lengths() {
        let call = act_call(&json!([{"do": "notify", "title": "Payroll", "message": "Salary file sent"}]));
        assert_eq!(quiet(&call), "notify title=<7 chars> message=<16 chars> return=diff");
        assert_eq!(verbose(&call), "notify title=<7 chars> message=<16 chars> return=diff");
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
            quiet(&call),
            "click e12 right x2; key <5 chars> down; mouse scroll e3 down 5; mouse drag e1 -> e2; focus w7; window close target=w7 return=none background"
        );
    }

    #[test]
    fn observe_names_the_mode_and_keeps_selectors() {
        let search = observe_call(json!({"query": "role:button name:Save", "all_windows": true}));
        assert_eq!(quiet(&search), "mode=elements query=role:button name:Save all_windows=true");
        assert_eq!(quiet(&observe_call(json!({"mode": "status"}))), "mode=status");
        let shot = observe_call(json!({"mode": "screenshot", "monitor": 1, "preset": "max", "marks": true}));
        assert_eq!(quiet(&shot), "mode=screenshot monitor=1 preset=max marks=true");
        let text = observe_call(json!({"mode": "text", "target": "e4", "ocr": "never"}));
        assert_eq!(quiet(&text), "mode=text target=e4 ocr=never");
    }

    #[test]
    fn recordings_keep_only_the_file_name() {
        let start = observe_call(json!({
            "mode": "record", "action": "start", "monitor": 1, "format": "gif",
            "path": "C:\\Users\\me\\Private\\demo.gif"
        }));
        let summary = verbose(&start);
        assert_eq!(summary, "mode=record monitor=1 format=gif action=start name=demo.gif");
        assert!(!summary.contains("Private"));
        assert_eq!(quiet(&observe_call(json!({"mode": "record", "action": "stop"}))), "mode=record action=stop");
    }

    #[test]
    fn listening_keeps_only_the_file_name_and_the_language() {
        let file = observe_call(json!({
            "mode": "listen", "path": "C:\\Users\\me\\Private\\call.m4a", "language": "hi", "translate": true
        }));
        let summary = verbose(&file);
        assert_eq!(summary, "mode=listen name=call.m4a language=hi translate=true");
        assert_eq!(quiet(&observe_call(json!({"mode": "listen", "audio": "mic"}))), "mode=listen audio=mic");
    }

    #[test]
    fn long_summaries_are_clipped() {
        let call = act_call(&json!([{"do": "focus", "target": "x".repeat(2000)}]));
        let summary = quiet(&call);
        assert_eq!(summary.chars().count(), MAX_SUMMARY_CHARS + 1);
        assert!(summary.ends_with('…'));
    }
}
