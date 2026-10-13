use std::time::Duration;

use deckhand_core::{
    Error, ErrorCode, Rect, Result, WindowInfo, WindowRef, WindowState, format::window_line, protocol::WindowAction,
};
use deckhand_desktop::Activation;

use super::{
    by_pattern::KeptForeground,
    done::Done,
    foreground::owner_chain,
    settle,
    stacking::keep_behind_front,
    window::{WindowRequest, state_word},
};
use crate::{call_scope::CallScope, engine::Engine, until::until, watch::Watch};

const STATE_WAIT: Duration = Duration::from_secs(1);
const KEPT_BEHIND: &str = "it stays";

pub(crate) fn perform(
    engine: &Engine,
    request: &WindowRequest<'_>,
    window: &WindowInfo,
    call: &CallScope,
) -> Result<Done> {
    require_user_elsewhere(window)?;
    let kept = KeptForeground::record(&engine.guard);
    let acted = act(engine, request, window, call);
    let handed_back = kept.hand_back(window.hwnd);
    let done = acted?.with_note(handed_back);
    if !restacks(request.action, window.state) {
        return Ok(done);
    }
    Ok(done.with_note(keep_behind_front(&deckhand_desktop::window(window.hwnd)?, KEPT_BEHIND)))
}

fn act(engine: &Engine, request: &WindowRequest<'_>, window: &WindowInfo, call: &CallScope) -> Result<Done> {
    match request.action {
        WindowAction::Minimize => change_state(engine, window, WindowState::Minimized, call),
        WindowAction::Maximize => change_state(engine, window, WindowState::Maximized, call),
        WindowAction::Restore => change_state(engine, window, WindowState::Normal, call),
        WindowAction::Move => {
            let rect = request.rect.ok_or_else(|| Error::invalid("window action=move needs \"rect\""))?;
            move_to(engine, window, rect, call)
        }
        WindowAction::Close | WindowAction::Launch => {
            Err(Error::invalid(format!("window {} does not change the window's state or place", request.action.name())))
        }
    }
}

fn change_state(engine: &Engine, window: &WindowInfo, state: WindowState, call: &CallScope) -> Result<Done> {
    deckhand_desktop::set_window_state(window.hwnd, state, Activation::KeepForeground)?;
    reach_state(engine, window, state, call)?;
    if state != WindowState::Minimized {
        let _early = keep_behind_front(window, KEPT_BEHIND);
    }
    settle(engine, window, call);
    let after = deckhand_desktop::window(window.hwnd)?;
    Ok(Done::noting(format!("{} {}", state_word(state), window_line(&after))))
}

fn reach_state(engine: &Engine, window: &WindowInfo, state: WindowState, call: &CallScope) -> Result<()> {
    let hwnd = window.hwnd;
    let _reached = until(engine, call, STATE_WAIT, &Watch::window(window), || {
        Ok((deckhand_desktop::window(hwnd)?.state == state).then_some(()))
    })?;
    Ok(())
}

fn move_to(engine: &Engine, window: &WindowInfo, rect: Rect, call: &CallScope) -> Result<Done> {
    deckhand_desktop::move_window(window.hwnd, rect, Activation::KeepForeground)?;
    settle(engine, window, call);
    let after = deckhand_desktop::window(window.hwnd)?;
    Ok(Done::noting(format!("moved {}", window_line(&after))))
}

const fn restacks(action: WindowAction, before: WindowState) -> bool {
    match action {
        WindowAction::Maximize | WindowAction::Restore => true,
        WindowAction::Move => matches!(before, WindowState::Minimized),
        WindowAction::Minimize | WindowAction::Close | WindowAction::Launch => false,
    }
}

fn require_user_elsewhere(window: &WindowInfo) -> Result<()> {
    let front = deckhand_desktop::foreground_window();
    if in_use(window.hwnd, front, &owner_chain(front)) {
        return Err(in_use_error(window));
    }
    Ok(())
}

fn in_use(target: WindowRef, front: Option<WindowRef>, owners_of_front: &[WindowRef]) -> bool {
    front == Some(target) || owners_of_front.contains(&target)
}

fn in_use_error(window: &WindowInfo) -> Error {
    Error::new(
        ErrorCode::NeedsUser,
        format!(
            "{} ({}) is the window the user is working in, so background mode leaves its size, place and state alone; drop background to change it",
            window.hwnd, window.process
        ),
    )
}

#[cfg(test)]
mod tests {
    use deckhand_core::protocol::{ActArgs, ActReport, AfterMode};
    use serde_json::{Value, json};

    use super::*;
    use crate::test_fixture::{ClosesTitled, FixtureApp, engine, fixture_exe, unique_title, window};

    fn act_behind(steps: Vec<Value>) -> ActReport {
        let steps = steps.into_iter().map(|value| serde_json::from_value(value).unwrap()).collect();
        let args = ActArgs { steps, after: AfterMode::None, stop_on_error: true, background: Some(true) };
        super::super::run(engine(), &args, &CallScope::relaxed()).unwrap()
    }

    fn assert_all_ok(report: &ActReport) {
        assert_eq!(report.steps.len(), report.total_steps, "the batch stopped early");
        for outcome in &report.steps {
            assert!(outcome.error.is_none(), "step {} failed: {:?}", outcome.index + 1, outcome.error);
        }
    }

    #[test]
    #[ignore = "opens the fixture app and changes its window behind the foreground window"]
    fn window_actions_keep_the_users_window_in_front() {
        let Some(app) = FixtureApp::launch_on_screen() else { return };
        let front = deckhand_desktop::foreground_window();
        let target = app.hwnd().to_string();
        let rect = json!({ "x": 160, "y": 140, "width": 720, "height": 520 });
        let report = act_behind(vec![
            json!({ "do": "window", "action": "minimize", "target": target }),
            json!({ "do": "window", "action": "restore", "target": target }),
            json!({ "do": "window", "action": "maximize", "target": target }),
            json!({ "do": "window", "action": "move", "target": target, "rect": rect }),
        ]);
        assert_all_ok(&report);
        assert_eq!(app.current_window().state, WindowState::Normal);
        assert_eq!(deckhand_desktop::foreground_window(), front, "the user's foreground window changed");
    }

    #[test]
    #[ignore = "opens the fixture app behind the foreground window"]
    fn launch_opens_behind_the_users_window() {
        let Some(exe) = fixture_exe() else { return };
        let title = unique_title("Behind");
        let _closes = ClosesTitled(title.clone());
        let front = deckhand_desktop::foreground_window();
        let args = format!("--title \"{title}\" --pos 120,120");
        let app = exe.to_string_lossy().into_owned();
        let report = act_behind(vec![json!({ "do": "window", "action": "launch", "app": app, "args": args })]);
        assert_all_ok(&report);
        assert!(report.steps[0].note.as_deref().unwrap_or_default().starts_with("launched "));
        assert_eq!(deckhand_desktop::foreground_window(), front, "the launched app took the foreground");
    }

    #[test]
    fn only_actions_that_can_raise_a_window_put_it_back_behind() {
        assert!(restacks(WindowAction::Maximize, WindowState::Normal));
        assert!(restacks(WindowAction::Restore, WindowState::Minimized));
        assert!(restacks(WindowAction::Move, WindowState::Minimized));
        assert!(!restacks(WindowAction::Move, WindowState::Normal));
        assert!(!restacks(WindowAction::Move, WindowState::Maximized));
        assert!(!restacks(WindowAction::Minimize, WindowState::Normal));
        assert!(!restacks(WindowAction::Close, WindowState::Normal));
    }

    #[test]
    fn the_users_window_and_the_owner_of_their_dialog_are_in_use() {
        assert!(in_use(WindowRef(5), Some(WindowRef(5)), &[]));
        assert!(in_use(WindowRef(5), Some(WindowRef(8)), &[WindowRef(5)]));
        assert!(!in_use(WindowRef(5), Some(WindowRef(8)), &[WindowRef(3)]));
        assert!(!in_use(WindowRef(5), None, &[]));
    }

    #[test]
    fn the_refusal_names_the_window_and_the_way_out() {
        let error = in_use_error(&window(7, "notepad.exe", "Notes", Rect::new(0, 0, 10, 10)));
        assert_eq!(error.code, ErrorCode::NeedsUser);
        assert!(
            error.message.starts_with("w7 (notepad.exe) is the window the user is working in"),
            "{}",
            error.message
        );
        assert!(error.message.ends_with("drop background to change it"), "{}", error.message);
    }
}
