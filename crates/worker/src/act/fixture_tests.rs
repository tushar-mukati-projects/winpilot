use deckhand_core::{
    ErrorCode, TargetSpec,
    diff::Change,
    protocol::{ActArgs, ActReport, After, AfterMode, ClickHow, Condition, MouseButton, Step, Until},
};

use super::{after::Baseline, run, select::select_by_pattern, target::prepare};
use crate::{
    call_scope::CallScope,
    engine::Engine,
    test_fixture::{FixtureApp, engine},
};

fn act(engine: &Engine, steps: Vec<Step>, stop_on_error: bool) -> ActReport {
    run(engine, &ActArgs { steps, after: AfterMode::None, stop_on_error }, &CallScope::relaxed()).unwrap()
}

fn target(fixture: &FixtureApp, terms: &str) -> TargetSpec {
    TargetSpec::Text(fixture.selector(terms))
}

fn click(fixture: &FixtureApp, terms: &str, how: ClickHow) -> Step {
    Step::Click {
        target: target(fixture, terms),
        button: MouseButton::Left,
        count: 1,
        modifiers: None,
        how,
        glide_ms: None,
    }
}

fn assert_all_ok(report: &ActReport) {
    assert_eq!(report.steps.len(), report.total_steps, "the batch stopped early");
    for outcome in &report.steps {
        assert!(outcome.error.is_none(), "step {} failed: {:?}", outcome.index + 1, outcome.error);
    }
}

fn assert_background(fixture: &FixtureApp) {
    assert!(deckhand_desktop::foreground_window() != Some(fixture.hwnd()), "the fixture took the foreground");
}

#[test]
#[ignore = "opens the fixture app off-screen"]
fn invoke_click_counts_without_taking_focus() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch() else { return };
    let report = act(engine, vec![click(&fixture, "id:2003", ClickHow::Invoke)], true);
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
    let report = act(engine, vec![click(&fixture, "id:2003", ClickHow::Auto)], true);
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
    let step = Step::SetValue { target: target(&fixture, "id:2001"), value: "hello deckhand".to_string() };
    let report = act(engine, vec![step], true);
    assert_all_ok(&report);
    assert_eq!(report.steps[0].method.as_deref(), Some("value"));
    assert_eq!(report.steps[0].note, None);
    assert_eq!(fixture.text(2001), "hello deckhand");
    assert_background(&fixture);
}

#[test]
#[ignore = "opens the fixture app off-screen"]
fn toggle_reaches_the_wanted_state_then_reports_already() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch() else { return };
    let checkbox = || target(&fixture, "id:2005");
    let report = act(engine, vec![Step::Toggle { target: checkbox(), on: Some(true) }], true);
    assert_all_ok(&report);
    assert_eq!(report.steps[0].method.as_deref(), Some("toggle"));
    assert_eq!(report.steps[0].note.as_deref(), Some("now on"));
    assert!(fixture.wait_for_text(2006, "Feature: on"), "feature is {:?}", fixture.text(2006));
    let again = act(engine, vec![Step::Toggle { target: checkbox(), on: Some(true) }], true);
    assert_all_ok(&again);
    assert_eq!(again.steps[0].method, None);
    assert_eq!(again.steps[0].note.as_deref(), Some("already on"));
    let flipped = act(engine, vec![Step::Toggle { target: checkbox(), on: None }], true);
    assert_all_ok(&flipped);
    assert_eq!(flipped.steps[0].note.as_deref(), Some("now off"));
    assert!(fixture.wait_for_text(2006, "Feature: off"), "feature is {:?}", fixture.text(2006));
    assert_background(&fixture);
}

#[test]
#[ignore = "opens the fixture app off-screen"]
fn select_by_pattern_picks_a_combobox_option_or_says_it_did_not_take() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch() else { return };
    let prepared = prepare(engine, &target(&fixture, "id:2007"), &CallScope::relaxed()).unwrap();
    let id = prepared.require_element("select").unwrap().id;
    match select_by_pattern(engine, &prepared, id, "Green", &CallScope::relaxed()) {
        Ok(done) => {
            assert_eq!(done.method.as_deref(), Some("select"));
            assert_eq!(done.note.as_deref(), Some("value now \"Green\""));
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
fn select_marks_a_list_item() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch() else { return };
    let item = target(&fixture, "id:2009 >> name:\"Item 3\"");
    let report = act(engine, vec![Step::Select { target: item, option: None }], true);
    assert_all_ok(&report);
    assert_eq!(report.steps[0].method.as_deref(), Some("select"));
    assert_eq!(report.steps[0].note.as_deref(), Some("selected \"Item 3\""));
    assert_background(&fixture);
}

#[test]
#[ignore = "opens the fixture app off-screen"]
fn wait_for_sees_the_delayed_button() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch() else { return };
    let appears = Condition {
        until: Until::Appears,
        target: Some("id:2016".to_string()),
        text: None,
        window: Some(fixture.title().to_string()),
        ms: None,
        timeout_ms: Some(4000),
    };
    let report = act(engine, vec![click(&fixture, "id:2015", ClickHow::Invoke), Step::WaitFor(appears)], true);
    assert_all_ok(&report);
    assert_eq!(report.steps[1].step, "wait_for appears id:2016");
    assert_eq!(report.steps[1].note.as_deref(), Some("id:2016 is visible"));
    assert_background(&fixture);
}

#[test]
#[ignore = "opens the fixture app off-screen"]
fn diff_reports_the_click_counter_change() {
    let engine = engine();
    let Some(fixture) = FixtureApp::launch() else { return };
    let baseline = Baseline::of(engine, fixture.current_window()).unwrap();
    let report = act(engine, vec![click(&fixture, "id:2003", ClickHow::Invoke)], true);
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
    let steps = vec![click(&fixture, "id:9999", ClickHow::Invoke), click(&fixture, "id:2003", ClickHow::Invoke)];
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
    let steps = vec![click(&fixture, "id:9999", ClickHow::Invoke), click(&fixture, "id:2003", ClickHow::Invoke)];
    let report = act(engine, steps, false);
    assert_eq!(report.steps.len(), 2);
    assert!(report.steps[0].error.is_some());
    assert!(report.steps[1].error.is_none(), "{:?}", report.steps[1].error);
    assert!(fixture.wait_for_text(2004, "Clicks: 1"), "click count is {:?}", fixture.text(2004));
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
    let args = ActArgs { steps: vec![Step::Wait { ms: 1 }; 51], after: AfterMode::None, stop_on_error: true };
    let error = run(engine, &args, &CallScope::relaxed()).unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidArgument);
}
