use deckhand_core::{
    ErrorCode, TargetSpec,
    protocol::{ActArgs, ActReport, AfterMode, ClickHow, Direction, MouseButton, Step, TypeHow},
};
use deckhand_desktop::clipboard;

use super::{mouse_select, run, target::prepare};
use crate::{
    call_scope::CallScope,
    engine::Engine,
    test_fixture::{FixtureApp, engine},
};

fn act(engine: &Engine, steps: Vec<Step>) -> ActReport {
    run(engine, &ActArgs { steps, after: AfterMode::None, stop_on_error: true }, &CallScope::relaxed()).unwrap()
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
    let click = |count, how| Step::Click {
        target: target(&fixture, "id:2003"),
        button: MouseButton::Left,
        count,
        modifiers: None,
        how,
        glide_ms: None,
    };
    let report = act(engine, vec![click(1, ClickHow::Mouse)]);
    assert_eq!(method(&report, 0), Some("mouse"));
    assert!(fixture.wait_for_text(2004, "Clicks: 1"));
    let report = act(engine, vec![click(2, ClickHow::Auto)]);
    assert_eq!(method(&report, 0), Some("mouse"));
    assert!(fixture.wait_for_text(2004, "Clicks: 3"));
}

#[test]
#[ignore = "moves the mouse and keyboard"]
fn typing_into_an_edit_reports_its_value() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch_on_screen() else { return };
    let step = Step::Type {
        target: Some(target(&fixture, "id:2001")),
        text: "typed by deckhand".to_string(),
        enter: false,
        how: TypeHow::Keys,
    };
    let report = act(engine, vec![step]);
    assert_eq!(method(&report, 0), Some("keys"));
    assert_eq!(report.steps[0].note.as_deref(), Some("value now \"typed by deckhand\""));
    assert_eq!(fixture.text(2001), "typed by deckhand");
}

#[test]
#[ignore = "moves the mouse and keyboard"]
fn long_text_is_pasted_and_the_clipboard_restored() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch_on_screen() else { return };
    let original = clipboard::get_text().unwrap();
    clipboard::set_text("deckhand sentinel").unwrap();
    let long = "pasted line ".repeat(30);
    let step =
        Step::Type { target: Some(target(&fixture, "id:2002")), text: long.clone(), enter: false, how: TypeHow::Auto };
    let report = act(engine, vec![step]);
    let restored = clipboard::get_text().unwrap();
    match &original {
        Some(text) => clipboard::set_text(text).unwrap(),
        None => clipboard::clear().unwrap(),
    }
    assert_eq!(method(&report, 0), Some("paste"));
    assert_eq!(restored.as_deref(), Some("deckhand sentinel"));
    assert_eq!(fixture.text(2002).trim_end(), long.trim_end());
}

#[test]
#[ignore = "moves the mouse and keyboard"]
fn keys_reach_the_focused_control() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch_on_screen() else { return };
    let notes = || Some(target(&fixture, "id:2002"));
    let report = act(engine, vec![Step::Key { keys: "ctrl+k".to_string(), target: notes(), repeat: 1 }]);
    assert_eq!(method(&report, 0), Some("keys"));
    assert!(fixture.wait_for_text(2020, "Last key: Ctrl+K"));
    let report = act(engine, vec![Step::Key { keys: "f5".to_string(), target: notes(), repeat: 2 }]);
    assert_eq!(method(&report, 0), Some("keys"));
    assert!(fixture.wait_for_text(2020, "Last key: F5"));
}

#[test]
#[ignore = "moves the mouse and keyboard"]
fn typing_without_a_target_goes_to_the_focused_app() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch_on_screen() else { return };
    let steps = vec![
        Step::Focus { target: target(&fixture, "id:2001") },
        Step::Type { target: None, text: "no target".to_string(), enter: false, how: TypeHow::Keys },
    ];
    let report = act(engine, steps);
    assert_eq!(method(&report, 0), Some("focus"));
    assert_eq!(method(&report, 1), Some("keys"));
    assert_eq!(fixture.text(2001), "no target");
}

#[test]
#[ignore = "moves the mouse and keyboard"]
fn scroll_hover_and_drag_use_the_mouse() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch_on_screen() else { return };
    let steps = vec![
        Step::Scroll {
            target: Some(target(&fixture, "id:2009")),
            direction: Direction::Down,
            amount: 5,
            glide_ms: None,
        },
        Step::Hover { target: target(&fixture, "id:2003"), glide_ms: None },
        Step::Drag { from: target(&fixture, "id:2001"), to: target(&fixture, "id:2002"), glide_ms: None },
    ];
    let report = act(engine, steps);
    assert_eq!(method(&report, 0), Some("wheel"));
    assert_eq!(method(&report, 1), Some("mouse"));
    assert_eq!(method(&report, 2), Some("mouse"));
}

#[test]
#[ignore = "moves the mouse and keyboard"]
fn focus_moves_keyboard_focus_into_the_edit() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch_on_screen() else { return };
    let report = act(engine, vec![Step::Focus { target: target(&fixture, "id:2001") }]);
    assert_eq!(method(&report, 0), Some("focus"));
    assert_eq!(report.steps[0].note, None);
    let focused = engine.uia.run(deckhand_uia::Uia::focused).unwrap();
    assert_eq!(focused.map(|element| element.automation_id), Some("2001".to_string()));
}

#[test]
#[ignore = "moves the mouse and keyboard"]
fn select_falls_back_to_clicking_the_combobox_open() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch_on_screen() else { return };
    let prepared = prepare(engine, &target(&fixture, "id:2007"), &CallScope::relaxed()).unwrap();
    let id = prepared.require_element("select").unwrap().id;
    let done = mouse_select::select(engine, &prepared, id, "blue", &CallScope::relaxed()).unwrap();
    assert_eq!(done.method.as_deref(), Some("mouse-select"));
    assert_eq!(done.note.as_deref(), Some("value now \"Blue\""));
    assert!(fixture.wait_for_text(2008, "Colour: Blue"));
}

#[test]
#[ignore = "moves the mouse and keyboard"]
fn act_select_reaches_the_option_by_pattern_or_mouse() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch_on_screen() else { return };
    let step = Step::Select { target: target(&fixture, "id:2007"), option: Some("Green".to_string()) };
    let report = act(engine, vec![step]);
    assert!(matches!(method(&report, 0), Some("select" | "mouse-select")));
    assert!(fixture.wait_for_text(2008, "Colour: Green"));
}

#[test]
#[ignore = "moves the mouse and keyboard"]
fn a_missing_option_closes_the_dropdown_and_lists_the_options() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch_on_screen() else { return };
    let prepared = prepare(engine, &target(&fixture, "id:2007"), &CallScope::relaxed()).unwrap();
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
    let prepared = prepare(engine, &target(&fixture, "id:2009"), &CallScope::relaxed()).unwrap();
    let id = prepared.require_element("select").unwrap().id;
    let done = mouse_select::select(engine, &prepared, id, "Item 2", &CallScope::relaxed()).unwrap();
    assert_eq!(done.method.as_deref(), Some("mouse-select"));
    assert_eq!(done.note.as_deref(), Some("selected \"Item 2\""));
    assert!(fixture.wait_for_text(2010, "Selected: Item 2"));
}
