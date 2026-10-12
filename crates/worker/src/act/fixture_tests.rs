use deckhand_core::{
    ErrorCode, TargetSpec,
    diff::Change,
    protocol::{ActArgs, ActReport, After, AfterMode, Step},
};
use serde_json::{Value, json};

use super::{after::Baseline, mode::Mode, run, select::select_by_pattern, target::prepare};
use crate::{
    call_scope::CallScope,
    engine::Engine,
    test_fixture::{FixtureApp, engine},
};

pub(crate) fn step(value: Value) -> Step {
    serde_json::from_value(value).unwrap()
}

fn act_with(engine: &Engine, steps: Vec<Step>, stop_on_error: bool, background: bool) -> ActReport {
    let args = ActArgs { steps, after: AfterMode::None, stop_on_error, background: Some(background) };
    run(engine, &args, &CallScope::relaxed()).unwrap()
}

pub(crate) fn act(engine: &Engine, steps: Vec<Step>, stop_on_error: bool) -> ActReport {
    act_with(engine, steps, stop_on_error, false)
}

fn click(fixture: &FixtureApp, terms: &str, how: &str) -> Step {
    step(json!({ "do": "click", "target": fixture.selector(terms), "how": how }))
}

fn assert_all_ok(report: &ActReport) {
    assert_eq!(report.steps.len(), report.total_steps, "the batch stopped early");
    for outcome in &report.steps {
        assert!(outcome.error.is_none(), "step {} failed: {:?}", outcome.index + 1, outcome.error);
    }
}

fn assert_background(fixture: &FixtureApp) {
    assert!(deckhand_desktop::foreground_window() != Some(fixture.hwnd()), "the fixture kept the foreground");
}

fn step_note(note: Option<&str>) -> Option<String> {
    let kept: Vec<&str> = note?.split("; ").filter(|part| !part.contains("took the foreground")).collect();
    (!kept.is_empty()).then(|| kept.join("; "))
}

#[test]
#[ignore = "opens the fixture app off-screen"]
fn invoke_click_counts_without_taking_focus() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch() else { return };
    let report = act(engine, vec![click(&fixture, "id:2003", "invoke")], true);
    assert_all_ok(&report);
    assert_eq!(report.steps[0].method.as_deref(), Some("invoke"));
    assert_eq!(report.steps[0].step, format!("click {}", fixture.selector("id:2003")));
    assert!(fixture.wait_for_text(2004, "Clicks: 1"), "click count is {:?}", fixture.text(2004));
    assert_background(&fixture);
}

#[test]
#[ignore = "opens the fixture app off-screen"]
fn auto_click_on_a_background_window_uses_the_pattern() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch() else { return };
    if fixture.current_window().foreground {
        eprintln!("skipping: the fixture is in the foreground, so auto would use the mouse");
        return;
    }
    let report = act(engine, vec![click(&fixture, "id:2003", "auto")], true);
    assert_all_ok(&report);
    assert_eq!(report.steps[0].method.as_deref(), Some("invoke"));
    assert!(fixture.wait_for_text(2004, "Clicks: 1"), "click count is {:?}", fixture.text(2004));
    assert_background(&fixture);
}

#[test]
#[ignore = "opens the fixture app off-screen"]
fn set_value_writes_the_edit_through_its_pattern() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch() else { return };
    let set = step(json!({ "do": "set", "target": fixture.selector("id:2001"), "value": "hello deckhand" }));
    let report = act(engine, vec![set], true);
    assert_all_ok(&report);
    assert_eq!(report.steps[0].method.as_deref(), Some("value"));
    assert_eq!(step_note(report.steps[0].note.as_deref()), None);
    assert_eq!(fixture.text(2001), "hello deckhand");
    assert_background(&fixture);
}

fn checked(fixture: &FixtureApp, on: bool) -> Step {
    step(json!({ "do": "set", "target": fixture.selector("id:2005"), "checked": on }))
}

fn toggled(report: &ActReport, fixture: &FixtureApp, word: &str) -> bool {
    let outcome = &report.steps[0];
    let label = format!("Feature: {word}");
    let Some(error) = &outcome.error else {
        assert_eq!(outcome.method.as_deref(), Some("toggle"));
        assert_eq!(step_note(outcome.note.as_deref()), Some(format!("now {word}")));
        assert!(fixture.wait_for_text(2006, &label), "feature is {:?}", fixture.text(2006));
        return true;
    };
    eprintln!("Windows kept the background fixture from activating, so its checkbox ignored the toggle");
    assert_eq!(error.code, ErrorCode::Unsupported);
    assert!(error.message.contains(&format!("should be {word}")), "{}", error.message);
    assert_ne!(fixture.text(2006), label, "the toggle took effect but was reported as failed");
    false
}

#[test]
#[ignore = "opens the fixture app off-screen"]
fn set_checked_reaches_the_wanted_state_or_says_it_did_not_take() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch() else { return };
    if toggled(&act(engine, vec![checked(&fixture, true)], true), &fixture, "on") {
        let again = act(engine, vec![checked(&fixture, true)], true);
        assert_all_ok(&again);
        assert_eq!(again.steps[0].method, None);
        assert_eq!(again.steps[0].note.as_deref(), Some("already on"));
        toggled(&act(engine, vec![checked(&fixture, false)], true), &fixture, "off");
    }
    assert_background(&fixture);
}

#[test]
#[ignore = "opens the fixture app off-screen"]
fn select_by_pattern_picks_a_combobox_option_or_says_it_did_not_take() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch() else { return };
    let combo = TargetSpec::Text(fixture.selector("id:2007"));
    let prepared = prepare(engine, &combo, Mode::Background, &CallScope::relaxed()).unwrap();
    let id = prepared.require_element("select").unwrap().id;
    match select_by_pattern(engine, &prepared, id, "Green", &CallScope::relaxed()) {
        Ok(done) => {
            assert_eq!(done.method.as_deref(), Some("select"));
            assert_eq!(step_note(done.note.as_deref()).as_deref(), Some("value now \"Green\""));
            assert!(fixture.wait_for_text(2008, "Colour: Green"), "colour is {:?}", fixture.text(2008));
        }
        Err(error) => {
            eprintln!("the pattern did not take on the background fixture: {error}");
            assert_eq!(error.code, ErrorCode::Unsupported);
            assert!(error.message.contains("did not take \"Green\""), "{}", error.message);
            assert_eq!(fixture.text(2008), "Colour: none");
        }
    }
    assert_background(&fixture);
}

#[test]
#[ignore = "opens the fixture app off-screen"]
fn set_selected_marks_a_list_item() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch() else { return };
    let item = step(json!({ "do": "set", "target": fixture.selector("id:2009 >> name:\"Item 3\""), "selected": true }));
    let report = act(engine, vec![item], true);
    assert_all_ok(&report);
    assert_eq!(report.steps[0].method.as_deref(), Some("select"));
    assert_eq!(step_note(report.steps[0].note.as_deref()).as_deref(), Some("selected \"Item 3\""));
    assert_background(&fixture);
}

#[test]
#[ignore = "opens the fixture app off-screen"]
fn wait_sees_the_delayed_button() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch() else { return };
    let appears = step(json!({
        "do": "wait", "until": "appears", "target": "id:2016", "window": fixture.title(), "timeout_ms": 4000
    }));
    let report = act(engine, vec![click(&fixture, "id:2015", "invoke"), appears], true);
    assert_all_ok(&report);
    assert_eq!(report.steps[1].step, "wait until id:2016 appears");
    assert_eq!(report.steps[1].note.as_deref(), Some("id:2016 is visible"));
    assert_background(&fixture);
}

#[test]
#[ignore = "opens the fixture app off-screen"]
fn diff_reports_the_click_counter_change() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch() else { return };
    let baseline = Baseline::of(engine, fixture.current_window()).unwrap();
    let report = act(engine, vec![click(&fixture, "id:2003", "invoke")], true);
    assert_all_ok(&report);
    assert!(fixture.wait_for_text(2004, "Clicks: 1"), "click count is {:?}", fixture.text(2004));
    let diff = baseline.compare(engine, fixture.current_window()).unwrap();
    assert_eq!(diff.window.hwnd, fixture.hwnd());
    let counter = diff.changes.iter().find_map(|change| match change {
        Change::Changed { before, after } if after.automation_id == "2004" => Some((before, after)),
        _ => None,
    });
    let Some((before, after)) = counter else { panic!("the click counter did not show up as changed") };
    assert_eq!((before.name.as_str(), after.name.as_str()), ("Clicks: 0", "Clicks: 1"));
    assert_background(&fixture);
}

#[test]
#[ignore = "opens the fixture app off-screen"]
fn a_missing_target_stops_the_batch() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch() else { return };
    let steps = vec![click(&fixture, "id:9999", "invoke"), click(&fixture, "id:2003", "invoke")];
    let report = act(engine, steps, true);
    assert_eq!((report.steps.len(), report.total_steps), (1, 2));
    assert_eq!(report.steps[0].error.as_ref().map(|error| error.code), Some(ErrorCode::NotFound));
    assert_eq!(fixture.text(2004), "Clicks: 0");
}

#[test]
#[ignore = "opens the fixture app off-screen"]
fn without_stop_on_error_the_rest_still_runs() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch() else { return };
    let steps = vec![click(&fixture, "id:9999", "invoke"), click(&fixture, "id:2003", "invoke")];
    let report = act(engine, steps, false);
    assert_eq!(report.steps.len(), 2);
    assert!(report.steps[0].error.is_some());
    assert!(report.steps[1].error.is_none(), "{:?}", report.steps[1].error);
    assert!(fixture.wait_for_text(2004, "Clicks: 1"), "click count is {:?}", fixture.text(2004));
}

#[test]
#[ignore = "opens the fixture app off-screen"]
fn background_mode_clicks_types_and_checks_through_patterns_only() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch() else { return };
    let cursor = deckhand_input::cursor_position().unwrap();
    let steps = vec![
        click(&fixture, "id:2003", "auto"),
        step(json!({ "do": "set", "target": fixture.selector("id:2001"), "value": "Ada" })),
        step(json!({ "do": "type", "target": fixture.selector("id:2001"), "text": " Lovelace" })),
    ];
    let report = act_with(engine, steps, true, true);
    assert_all_ok(&report);
    assert_eq!(report.steps[0].method.as_deref(), Some("invoke"));
    assert_eq!(report.steps[2].method.as_deref(), Some("value"));
    assert!(report.steps[2].note.as_deref().unwrap_or_default().starts_with("appended"), "{:?}", report.steps[2]);
    assert!(fixture.wait_for_text(2004, "Clicks: 1"), "click count is {:?}", fixture.text(2004));
    assert_eq!(fixture.text(2001), "Ada Lovelace");
    toggled(&act_with(engine, vec![checked(&fixture, true)], true, true), &fixture, "on");
    assert_eq!(deckhand_input::cursor_position().unwrap(), cursor, "background mode moved the mouse");
    assert_background(&fixture);
}

#[test]
#[ignore = "opens the fixture app off-screen"]
fn background_mode_refuses_real_input_steps() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch() else { return };
    let key = step(json!({ "do": "key", "keys": "ctrl+f5", "target": fixture.selector("id:2002") }));
    let report = act_with(engine, vec![key], true, true);
    let error = report.steps[0].error.as_ref().unwrap();
    assert_eq!(error.code, ErrorCode::NeedsUser);
    assert_eq!(
        error.message,
        "a key chord with modifiers needs the real keyboard, because apps read ctrl, alt, shift and win from it; background mode never touches it (drop background)"
    );
    assert_eq!(fixture.text(2020), "Last key: none");
    assert_background(&fixture);
}

#[test]
#[ignore = "starts a full engine"]
fn an_empty_batch_reports_only_the_after_state() {
    let engine = engine();
    let report = act(engine, Vec::new(), true);
    assert!(report.steps.is_empty());
    assert_eq!(report.total_steps, 0);
    assert_eq!(report.after, After::None);
}

#[test]
#[ignore = "starts a full engine"]
fn too_many_steps_are_refused_before_anything_runs() {
    let engine = engine();
    let pause = step(json!({ "do": "wait", "ms": 1 }));
    let args = ActArgs { steps: vec![pause; 51], after: AfterMode::None, stop_on_error: true, background: None };
    let error = run(engine, &args, &CallScope::relaxed()).unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidArgument);
}
