use std::{thread, time::Duration};

use deckhand_core::{
    Result,
    protocol::{Until, WaitArgs},
};

use super::{WaitReport, WindowsBefore, until};
use crate::{
    call_scope::CallScope,
    engine::Engine,
    test_fixture::{FixtureApp, engine, unique_title},
};

fn run(engine: &Engine, args: &WaitArgs, call: &CallScope) -> Result<WaitReport> {
    until(engine, &args.validate()?, args.timeout(), &WindowsBefore::now(engine), call)
}

fn condition(until: Until, fixture: &FixtureApp) -> WaitArgs {
    let window = Some(fixture.title().to_string());
    WaitArgs { until: Some(until), window, timeout_ms: Some(300), ..WaitArgs::default() }
}

#[test]
#[ignore = "opens the fixture app off-screen"]
fn text_finds_the_click_counter() {
    let Some(fixture) = FixtureApp::launch() else { return };
    let shown = WaitArgs { text: Some("clicks: 0".to_string()), ..condition(Until::Text, &fixture) };
    let report = run(engine(), &shown, &CallScope::relaxed()).unwrap();
    assert!(report.satisfied);
    assert_eq!(report.element.map(|element| element.automation_id), Some("2004".to_string()));
}

#[test]
#[ignore = "opens the fixture app off-screen"]
fn a_missing_element_times_out_and_counts_as_gone() {
    let Some(fixture) = FixtureApp::launch() else { return };
    let appears = WaitArgs { target: Some("id:9999".to_string()), ..condition(Until::Appears, &fixture) };
    let report = run(engine(), &appears, &CallScope::relaxed()).unwrap();
    assert!(!report.satisfied);
    assert!(report.elapsed_ms >= 300, "gave up after {} ms", report.elapsed_ms);
    assert_eq!(report.detail, "id:9999 has not appeared");
    let disappears = WaitArgs { target: Some("id:9999".to_string()), ..condition(Until::Disappears, &fixture) };
    let report = run(engine(), &disappears, &CallScope::relaxed()).unwrap();
    assert!(report.satisfied);
    assert!(report.elapsed_ms < 300, "took {} ms", report.elapsed_ms);
}

#[test]
#[ignore = "opens the fixture app off-screen"]
fn a_visible_element_appears_at_once() {
    let Some(fixture) = FixtureApp::launch() else { return };
    let appears = WaitArgs { target: Some("id:2003".to_string()), ..condition(Until::Appears, &fixture) };
    let report = run(engine(), &appears, &CallScope::relaxed()).unwrap();
    assert!(report.satisfied);
    assert_eq!(report.element.map(|element| element.name), Some("Click me".to_string()));
}

#[test]
#[ignore = "opens the fixture app off-screen"]
fn window_conditions_track_the_fixture() {
    let Some(fixture) = FixtureApp::launch() else { return };
    let opens = condition(Until::Window, &fixture);
    let already = run(engine(), &opens, &CallScope::relaxed()).unwrap();
    assert!(!already.satisfied, "a window open before the wait counted as newly opened");
    assert!(already.detail.ends_with("was already open before the wait)"), "{}", already.detail);
    let closes = condition(Until::WindowClosed, &fixture);
    let report = run(engine(), &closes, &CallScope::relaxed()).unwrap();
    assert!(!report.satisfied);
    assert!(report.detail.ends_with("is still open"));
    let title = fixture.title().to_string();
    drop(fixture);
    let after_close = WaitArgs { window: Some(title), timeout_ms: Some(2000), ..closes };
    let report = run(engine(), &after_close, &CallScope::relaxed()).unwrap();
    assert!(report.satisfied, "the closed fixture still matched");
}

#[test]
#[ignore = "opens the fixture app off-screen"]
fn a_cancelled_wait_stops() {
    let Some(fixture) = FixtureApp::launch() else { return };
    let cancel = crate::cancel::Cancel::default();
    cancel.cancel();
    let cancelled = CallScope::new(cancel, std::time::Instant::now() + Duration::from_secs(60));
    let appears = WaitArgs {
        target: Some("id:9999".to_string()),
        timeout_ms: Some(Duration::from_secs(5).as_millis() as u64),
        ..condition(Until::Appears, &fixture)
    };
    let error = run(engine(), &appears, &cancelled).unwrap_err();
    assert_eq!(error.code, deckhand_core::ErrorCode::Cancelled);
}

#[test]
#[ignore = "opens the fixture app off-screen"]
fn a_window_opened_during_the_wait_satisfies_window_opens() {
    let title = unique_title("Opened");
    let opens = WaitArgs {
        until: Some(Until::Window),
        window: Some(title.clone()),
        timeout_ms: Some(8000),
        ..WaitArgs::default()
    };
    let waiter = thread::spawn(move || run(engine(), &opens, &CallScope::relaxed()));
    thread::sleep(Duration::from_millis(300));
    let fixture = FixtureApp::launch_titled(title);
    let report = waiter.join().unwrap().unwrap();
    if fixture.is_none() {
        return;
    }
    assert!(report.satisfied, "{}", report.detail);
}
