use std::time::Duration;

use deckhand_core::{
    Error, ErrorCode, Rect, Result, TargetSpec, WindowInfo, WindowState,
    format::window_line,
    protocol::{Step, WindowAction},
};

use super::{
    done::Done,
    launch,
    mode::{Mode, for_window},
    settle,
    steps::StepContext,
};
use crate::{call_scope::CallScope, engine::Engine, overlay, resolve, until::until, watch::Watch};

const CLOSE_WAIT: Duration = Duration::from_secs(3);
const LISTED_MATCHES: usize = 8;

pub struct WindowRequest<'a> {
    pub action: WindowAction,
    pub target: Option<&'a TargetSpec>,
    pub app: Option<&'a str>,
    pub args: Option<&'a str>,
    pub rect: Option<Rect>,
}

pub fn step(engine: &Engine, request: &WindowRequest<'_>, step: &Step, context: &StepContext<'_>) -> Result<Done> {
    let call = context.call;
    if request.action == WindowAction::Launch {
        let app = request.app.ok_or_else(|| Error::invalid("window action=launch needs \"app\""))?;
        return launch::launch(engine, app, request.args, call);
    }
    let target = request.target.ok_or_else(|| Error::invalid("this window action needs \"target\""))?;
    let window = target_window(engine, target, call)?;
    let mode = for_window(context.mode, &window, step)?;
    guard(engine, &window, mode, call)?;
    overlay::begin(engine, &window, call);
    let done = perform(engine, request, &window, call)?;
    let after = deckhand_desktop::window(window.hwnd).ok();
    if let Some(after) = after.filter(|after| after.state != WindowState::Minimized) {
        overlay::begin(engine, &after, call);
    }
    Ok(done)
}

fn perform(engine: &Engine, request: &WindowRequest<'_>, window: &WindowInfo, call: &CallScope) -> Result<Done> {
    match request.action {
        WindowAction::Close => close(engine, window, call),
        WindowAction::Minimize => change_state(engine, window, WindowState::Minimized, call),
        WindowAction::Maximize => change_state(engine, window, WindowState::Maximized, call),
        WindowAction::Restore => change_state(engine, window, WindowState::Normal, call),
        WindowAction::Move => {
            let rect = request.rect.ok_or_else(|| Error::invalid("window action=move needs \"rect\""))?;
            move_to(engine, window, rect, call)
        }
        WindowAction::Launch => Err(Error::invalid("window action=launch takes app, not a target")),
    }
}

fn guard(engine: &Engine, window: &WindowInfo, mode: Mode, call: &CallScope) -> Result<()> {
    match mode {
        Mode::Foreground => engine.guard.before_input(Some(window), call),
        Mode::Background => engine.guard.before_patterns(Some(window), call),
    }
}

fn target_window(engine: &Engine, target: &TargetSpec, call: &CallScope) -> Result<WindowInfo> {
    let window = match target {
        TargetSpec::Text(spec) => single_window(spec.trim(), resolve::windows_matching(engine, spec.trim())?)?,
        TargetSpec::Point(_) => resolve::target(engine, &target.parse()?, call)?.window,
    };
    engine.guard.check_window(&window)?;
    engine.host.refuse(&window)?;
    Ok(window)
}

fn close(engine: &Engine, window: &WindowInfo, call: &CallScope) -> Result<Done> {
    deckhand_desktop::close_window(window.hwnd)?;
    let hwnd = window.hwnd;
    let closed = until(engine, call, CLOSE_WAIT, &Watch::window(window), || {
        Ok((!deckhand_desktop::is_window(hwnd)).then_some(()))
    })?;
    if closed.is_some() {
        return Ok(Done::noting(format!("closed {}", window_line(window))));
    }
    Ok(Done::noting(format!("{} is still open; it may be asking something", window.hwnd)))
}

fn change_state(engine: &Engine, window: &WindowInfo, state: WindowState, call: &CallScope) -> Result<Done> {
    deckhand_desktop::set_window_state(window.hwnd, state)?;
    settle(engine, window, call);
    let after = deckhand_desktop::window(window.hwnd)?;
    Ok(Done::noting(format!("{} {}", state_word(state), window_line(&after))))
}

const fn state_word(state: WindowState) -> &'static str {
    match state {
        WindowState::Minimized => "minimized",
        WindowState::Maximized => "maximized",
        WindowState::Normal => "restored",
    }
}

fn move_to(engine: &Engine, window: &WindowInfo, rect: Rect, call: &CallScope) -> Result<Done> {
    deckhand_desktop::move_window(window.hwnd, rect)?;
    settle(engine, window, call);
    let after = deckhand_desktop::window(window.hwnd)?;
    Ok(Done::noting(format!("moved {}", window_line(&after))))
}

pub fn single_window(spec: &str, found: Vec<WindowInfo>) -> Result<WindowInfo> {
    let count = found.len();
    let mut found = found.into_iter();
    match (found.next(), count) {
        (None, _) => Err(Error::not_found(format!("no open window matches {spec}"))),
        (Some(only), 1) => Ok(only),
        (Some(first), _) => Err(ambiguous(spec, count, std::iter::once(first).chain(found))),
    }
}

fn ambiguous(spec: &str, count: usize, windows: impl Iterator<Item = WindowInfo>) -> Error {
    let mut lines: Vec<String> = windows.take(LISTED_MATCHES).map(|window| window_line(&window)).collect();
    if count > LISTED_MATCHES {
        lines.push("…".to_string());
    }
    let listed = lines.join("; ");
    Error::new(ErrorCode::Ambiguous, format!("{count} windows match {spec}: {listed}; pass one window ref"))
}

#[cfg(test)]
mod tests {
    use deckhand_core::WindowRef;

    use super::*;
    use crate::test_fixture::window;

    #[test]
    fn only_a_single_matching_window_can_be_controlled() {
        let first = window(1, "chrome.exe", "Inbox", Rect::new(0, 0, 10, 10));
        let second = window(2, "chrome.exe", "Docs", Rect::new(0, 0, 10, 10));
        assert_eq!(single_window("process:chrome", vec![first.clone()]).unwrap().hwnd, WindowRef(1));
        let error = single_window("process:chrome", vec![first, second]).unwrap_err();
        assert_eq!(error.code, ErrorCode::Ambiguous);
        assert!(
            error.message.starts_with("2 windows match process:chrome: w1 chrome.exe \"Inbox\"; w2"),
            "{}",
            error.message
        );
        assert_eq!(single_window("Nothing", Vec::new()).unwrap_err().code, ErrorCode::NotFound);
    }

    #[test]
    fn long_ambiguity_lists_are_capped() {
        let many: Vec<WindowInfo> = (1..=10).map(|hwnd| window(hwnd, "app.exe", "A", Rect::default())).collect();
        let message = single_window("A", many).unwrap_err().message;
        assert!(message.contains("w8 app.exe \"A\"; …; pass one window ref"), "{message}");
        assert!(!message.contains("w9 "));
    }

    #[test]
    fn state_changes_are_named() {
        assert_eq!(state_word(WindowState::Minimized), "minimized");
        assert_eq!(state_word(WindowState::Normal), "restored");
    }
}
