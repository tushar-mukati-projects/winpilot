use serde_json::json;
use winpilot_core::{
    ErrorCode, Rect, TargetSpec, WindowState,
    protocol::{ActReport, Step},
};
use winpilot_desktop::clipboard;

use super::{
    fixture_tests::{self, step},
    mode::Mode,
    mouse_select,
    target::prepare,
};
use crate::{
    call_scope::CallScope,
    engine::Engine,
    test_fixture::{ClosesTitled, FixtureApp, engine, fixture_exe, unique_title},
};

fn act(engine: &Engine, steps: Vec<Step>) -> ActReport {
    fixture_tests::act(engine, steps, true)
}

fn target(fixture: &FixtureApp, terms: &str) -> TargetSpec {
    TargetSpec::Text(fixture.selector(terms))
}

fn method(report: &ActReport, index: usize) -> Option<&str> {
    let outcome = &report.steps[index];
    assert!(outcome.error.is_none(), "step {} failed: {:?}", index + 1, outcome.error);
    outcome.method.as_deref()
}

#[test]
#[ignore = "moves the mouse and keyboard"]
fn mouse_clicks_count_single_and_double() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch_on_screen() else { return };
    let click = |count: u8, how: &str| {
        step(json!({ "do": "click", "target": fixture.selector("id:2003"), "count": count, "how": how }))
    };
    let report = act(engine, vec![click(1, "mouse")]);
    assert_eq!(method(&report, 0), Some("mouse"));
    assert!(fixture.wait_for_text(2004, "Clicks: 1"));
    let report = act(engine, vec![click(2, "auto")]);
    assert_eq!(method(&report, 0), Some("mouse"));
    assert!(fixture.wait_for_text(2004, "Clicks: 3"));
}

#[test]
#[ignore = "moves the mouse and keyboard"]
fn typing_into_an_edit_reports_its_value() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch_on_screen() else { return };
    let typed = step(
        json!({ "do": "type", "target": fixture.selector("id:2001"), "text": "typed by winpilot", "how": "keys" }),
    );
    let report = act(engine, vec![typed]);
    assert_eq!(method(&report, 0), Some("keys"));
    assert_eq!(report.steps[0].note.as_deref(), Some("value now \"typed by winpilot\""));
    assert_eq!(fixture.text(2001), "typed by winpilot");
}

#[test]
#[ignore = "moves the mouse and keyboard"]
fn long_text_is_pasted_and_the_clipboard_restored() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch_on_screen() else { return };
    let original = clipboard::get_text().unwrap();
    clipboard::set_text("winpilot sentinel").unwrap();
    let long = "pasted line ".repeat(30);
    let report = act(engine, vec![step(json!({ "do": "type", "target": fixture.selector("id:2002"), "text": long }))]);
    let restored = clipboard::get_text().unwrap();
    match &original {
        Some(text) => clipboard::set_text(text).unwrap(),
        None => clipboard::clear().unwrap(),
    }
    assert_eq!(method(&report, 0), Some("paste"));
    assert_eq!(restored.as_deref(), Some("winpilot sentinel"));
    assert_eq!(fixture.text(2002).trim_end(), long.trim_end());
}

#[test]
#[ignore = "moves the mouse and keyboard"]
fn keys_reach_the_focused_control() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch_on_screen() else { return };
    let key = |keys: &str, repeat: u32| {
        step(json!({ "do": "key", "keys": keys, "target": fixture.selector("id:2002"), "repeat": repeat }))
    };
    let report = act(engine, vec![key("ctrl+k", 1)]);
    assert_eq!(method(&report, 0), Some("keys"));
    assert!(fixture.wait_for_text(2020, "Last key: Ctrl+K"));
    let report = act(engine, vec![key("f5", 2)]);
    assert_eq!(method(&report, 0), Some("keys"));
    assert!(fixture.wait_for_text(2020, "Last key: F5"));
}

#[test]
#[ignore = "moves the mouse and keyboard"]
fn held_keys_are_released_at_the_end_of_the_call() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch_on_screen() else { return };
    let steps = vec![
        step(json!({ "do": "key", "keys": "shift", "action": "down", "target": fixture.selector("id:2002") })),
        step(json!({ "do": "key", "keys": "ctrl", "action": "down" })),
        step(json!({ "do": "key", "keys": "ctrl", "action": "up" })),
    ];
    let report = act(engine, steps);
    assert_eq!(method(&report, 0), Some("keys"));
    assert_eq!(report.steps[2].note.as_deref(), Some("released Shift left held by key down"));
}

#[test]
#[ignore = "moves the mouse and keyboard"]
fn a_long_wait_is_refused_while_a_key_is_held() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch_on_screen() else { return };
    let steps = vec![
        step(json!({ "do": "key", "keys": "shift", "action": "down", "target": fixture.selector("id:2002") })),
        step(json!({ "do": "wait", "ms": 1500 })),
    ];
    let report = act(engine, steps);
    assert_eq!(method(&report, 0), Some("keys"));
    let refused = &report.steps[1];
    assert_eq!(refused.error.as_ref().map(|error| error.code), Some(ErrorCode::InvalidArgument));
    assert_eq!(refused.note.as_deref(), Some("released Shift left held by key down"));
}

#[test]
#[ignore = "moves the mouse and keyboard"]
fn typing_without_a_target_goes_to_the_focused_app() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch_on_screen() else { return };
    let steps = vec![
        step(json!({ "do": "focus", "target": fixture.selector("id:2001") })),
        step(json!({ "do": "type", "text": "no target", "how": "keys" })),
    ];
    let report = act(engine, steps);
    assert_eq!(method(&report, 0), Some("focus"));
    assert_eq!(method(&report, 1), Some("keys"));
    assert_eq!(fixture.text(2001), "no target");
}

#[test]
#[ignore = "moves the mouse and keyboard"]
fn scroll_move_and_drag_use_the_mouse() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch_on_screen() else { return };
    let steps = vec![
        step(
            json!({ "do": "mouse", "action": "scroll", "target": fixture.selector("id:2009"), "direction": "down", "amount": 5 }),
        ),
        step(json!({ "do": "mouse", "action": "move", "target": fixture.selector("id:2003") })),
        step(
            json!({ "do": "mouse", "action": "drag", "target": fixture.selector("id:2001"), "to": fixture.selector("id:2002") }),
        ),
    ];
    let report = act(engine, steps);
    assert_eq!(method(&report, 0), Some("wheel"));
    assert_eq!(method(&report, 1), Some("mouse"));
    assert_eq!(method(&report, 2), Some("mouse"));
}

#[test]
#[ignore = "moves the mouse and keyboard"]
fn a_held_mouse_button_is_released_at_the_end_of_the_call() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch_on_screen() else { return };
    let down = step(json!({ "do": "mouse", "action": "down", "target": fixture.selector("id:2001") }));
    let report = act(engine, vec![down]);
    assert_eq!(method(&report, 0), Some("mouse"));
    assert_eq!(report.steps[0].note.as_deref(), Some("released the left mouse button left held by mouse down"));
}

#[test]
#[ignore = "moves the mouse and keyboard"]
fn a_button_held_when_the_call_fails_is_released_where_it_was_pressed() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch_on_screen() else { return };
    let steps = vec![
        step(json!({ "do": "mouse", "action": "down", "target": fixture.selector("id:2001") })),
        step(json!({ "do": "mouse", "action": "move", "target": fixture.selector("id:2002") })),
        step(json!({ "do": "click", "target": fixture.selector("id:9999") })),
    ];
    let report = act(engine, steps);
    assert_eq!(method(&report, 1), Some("mouse"));
    let failed = &report.steps[2];
    assert_eq!(failed.error.as_ref().map(|error| error.code), Some(ErrorCode::NotFound));
    let note = failed.note.as_deref().unwrap_or_default();
    assert!(note.ends_with("back where it was pressed because the call ended early"), "{note}");
}

#[test]
#[ignore = "moves the mouse and keyboard"]
fn focus_moves_keyboard_focus_into_the_edit() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch_on_screen() else { return };
    let report = act(engine, vec![step(json!({ "do": "focus", "target": fixture.selector("id:2001") }))]);
    assert_eq!(method(&report, 0), Some("focus"));
    assert_eq!(report.steps[0].note, None);
    let focused = engine.uia.run(winpilot_uia::Uia::focused).unwrap();
    assert_eq!(focused.map(|element| element.automation_id), Some("2001".to_string()));
}

#[test]
#[ignore = "moves the mouse and keyboard"]
fn select_falls_back_to_clicking_the_combobox_open() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch_on_screen() else { return };
    let prepared = prepare(engine, &target(&fixture, "id:2007"), Mode::Foreground, &CallScope::relaxed()).unwrap();
    let id = prepared.require_element("select").unwrap().id;
    let done = mouse_select::select(engine, &prepared, id, "blue", &CallScope::relaxed()).unwrap();
    assert_eq!(done.method.as_deref(), Some("mouse-select"));
    assert_eq!(done.note.as_deref(), Some("value now \"Blue\""));
    assert!(fixture.wait_for_text(2008, "Colour: Blue"));
}

#[test]
#[ignore = "moves the mouse and keyboard"]
fn set_option_reaches_the_option_by_pattern_or_mouse() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch_on_screen() else { return };
    let report =
        act(engine, vec![step(json!({ "do": "set", "target": fixture.selector("id:2007"), "option": "Green" }))]);
    assert!(matches!(method(&report, 0), Some("select" | "mouse-select")));
    assert!(fixture.wait_for_text(2008, "Colour: Green"));
}

#[test]
#[ignore = "moves the mouse and keyboard"]
fn a_missing_option_closes_the_dropdown_and_lists_the_options() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch_on_screen() else { return };
    let prepared = prepare(engine, &target(&fixture, "id:2007"), Mode::Foreground, &CallScope::relaxed()).unwrap();
    let id = prepared.require_element("select").unwrap().id;
    let error = mouse_select::select(engine, &prepared, id, "Purple", &CallScope::relaxed()).unwrap_err();
    assert_eq!(error.code, ErrorCode::NotFound);
    assert!(error.message.contains("\"Red\", \"Green\", \"Blue\""), "{}", error.message);
    let combobox = engine.uia.run(move |uia| uia.element(id)).unwrap();
    assert_eq!(combobox.state.expanded, Some(false));
    assert_eq!(fixture.text(2008), "Colour: none");
}

#[test]
#[ignore = "moves the mouse and keyboard"]
fn a_list_option_is_clicked_directly() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch_on_screen() else { return };
    let prepared = prepare(engine, &target(&fixture, "id:2009"), Mode::Foreground, &CallScope::relaxed()).unwrap();
    let id = prepared.require_element("select").unwrap().id;
    let done = mouse_select::select(engine, &prepared, id, "Item 2", &CallScope::relaxed()).unwrap();
    assert_eq!(done.method.as_deref(), Some("mouse-select"));
    assert_eq!(done.note.as_deref(), Some("selected \"Item 2\""));
    assert!(fixture.wait_for_text(2010, "Selected: Item 2"));
}

fn window_step(fixture: &FixtureApp, action: &str) -> Step {
    step(json!({ "do": "window", "action": action, "target": fixture.hwnd().to_string() }))
}

#[test]
#[ignore = "opens dialogs / changes focus"]
fn minimize_and_restore_the_fixture() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch() else { return };
    act(engine, vec![window_step(&fixture, "minimize")]);
    assert_eq!(fixture.current_window().state, WindowState::Minimized);
    act(engine, vec![window_step(&fixture, "restore")]);
    assert_eq!(fixture.current_window().state, WindowState::Normal);
}

#[test]
#[ignore = "opens dialogs / changes focus"]
fn move_the_fixture() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch() else { return };
    let wanted = Rect::new(fixture.window.rect.x + 40, fixture.window.rect.y + 30, 500, 400);
    let moved = step(json!({ "do": "window", "action": "move", "target": fixture.hwnd().to_string(), "rect": wanted }));
    act(engine, vec![moved]);
    assert_eq!(fixture.current_window().rect, wanted);
}

#[test]
#[ignore = "opens dialogs / changes focus"]
fn close_the_fixture() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch() else { return };
    let report = act(engine, vec![window_step(&fixture, "close")]);
    assert!(report.steps[0].note.as_deref().unwrap_or_default().starts_with("closed "));
    assert!(!winpilot_desktop::is_window(fixture.hwnd()));
}

#[test]
#[ignore = "opens dialogs / changes focus"]
fn launch_the_fixture_by_path() {
    let engine = engine();
    let Some(exe) = fixture_exe() else { return };
    let exe = std::fs::canonicalize(exe).unwrap();
    let title = unique_title("Launch");
    let _close = ClosesTitled(title.clone());
    let app = exe.to_string_lossy().trim_start_matches("\\\\?\\").to_string();
    let launch =
        step(json!({ "do": "window", "action": "launch", "app": app, "args": format!("--title \"{title}\"") }));
    let report = act(engine, vec![launch]);
    assert!(report.steps[0].note.as_deref().unwrap_or_default().contains(&title));
}
