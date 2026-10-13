use std::time::Duration;

use deckhand_core::{
    Error, ErrorCode, Rect, Result, TargetSpec, WindowInfo, WindowState,
    format::window_line,
    protocol::{Step, WindowAction},
};
use deckhand_desktop::Activation;

use super::{
    done::Done,
    launch,
    mode::{Mode, check_before, for_window},
    settle,
    steps::StepContext,
    window_behind,
};
use crate::{
    app_windows::app_windows, call_scope::CallScope, engine::Engine, overlay, resolve, until::until, watch::Watch,
    window_classes::is_shell,
};

const CLOSE_WAIT: Duration = Duration::from_secs(3);

pub(crate) struct WindowRequest<'a> {
    pub(crate) action: WindowAction,
    pub(crate) target: Option<&'a TargetSpec>,
    pub(crate) app: Option<&'a str>,
    pub(crate) args: Option<&'a str>,
    pub(crate) rect: Option<Rect>,
}

impl<'a> TryFrom<&'a Step> for WindowRequest<'a> {
    type Error = Error;

    fn try_from(step: &'a Step) -> Result<Self> {
        let Step::Window { action, target, app, args, rect } = step else {
            return Err(Error::invalid(format!("{} is not a window step", step.kind())));
        };
        Ok(Self { action: *action, target: target.as_ref(), app: app.as_deref(), args: args.as_deref(), rect: *rect })
    }
}

pub(crate) fn step(
    engine: &Engine,
    request: &WindowRequest<'_>,
    step: &Step,
    context: &StepContext<'_>,
) -> Result<Done> {
    let call = context.call;
    if request.action == WindowAction::Launch {
        let app = request.app.ok_or_else(|| Error::invalid("window action=launch needs \"app\""))?;
        return launch::launch(engine, app, request.args, context.mode, call);
    }
    let target = request.target.ok_or_else(|| Error::invalid("this window action needs \"target\""))?;
    let window = target_window(engine, target, call)?;
    let mode = for_window(context.mode, &window, step)?;
    check_before(engine, Some(&window), mode, call)?;
    overlay::begin(engine, &window, call);
    let done = match mode {
        Mode::Background if request.action != WindowAction::Close => {
            window_behind::perform(engine, request, &window, call)?
        }
        Mode::Background | Mode::Foreground => perform(engine, request, &window, call)?,
    };
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

fn target_window(engine: &Engine, target: &TargetSpec, call: &CallScope) -> Result<WindowInfo> {
    let window = match target {
        TargetSpec::Text(spec) => single_window(spec.trim(), resolve::windows_matching(engine, spec.trim())?)?,
        TargetSpec::Point(_) => resolve::target(engine, &target.parse()?, call)?.window,
    };
    engine.guard.check_window(&window)?;
    engine.host.refuse(&window)?;
    require_app_window(&window, &app_windows(engine)?)?;
    Ok(window)
}

fn require_app_window(window: &WindowInfo, listed: &[WindowInfo]) -> Result<()> {
    if is_shell(window) {
        return Err(Error::new(
            ErrorCode::Denied,
            format!(
                "{} ({}) is part of the Windows shell; window actions only control app windows",
                window.hwnd, window.class_name
            ),
        ));
    }
    if listed.iter().all(|app| app.hwnd != window.hwnd) {
        return Err(Error::not_found(format!(
            "{} is not an app window Deckhand lists (observe mode=windows shows them)",
            window.hwnd
        )));
    }
    Ok(())
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
    deckhand_desktop::set_window_state(window.hwnd, state, Activation::Activate)?;
    settle(engine, window, call);
    let after = deckhand_desktop::window(window.hwnd)?;
    Ok(Done::noting(format!("{} {}", state_word(state), window_line(&after))))
}

pub(crate) const fn state_word(state: WindowState) -> &'static str {
    match state {
        WindowState::Minimized => "minimized",
        WindowState::Maximized => "maximized",
        WindowState::Normal => "restored",
    }
}

fn move_to(engine: &Engine, window: &WindowInfo, rect: Rect, call: &CallScope) -> Result<Done> {
    deckhand_desktop::move_window(window.hwnd, rect, Activation::Activate)?;
    settle(engine, window, call);
    let after = deckhand_desktop::window(window.hwnd)?;
    Ok(Done::noting(format!("moved {}", window_line(&after))))
}

fn single_window(spec: &str, found: Vec<WindowInfo>) -> Result<WindowInfo> {
    let mut found = found;
    match found.len() {
        0 => Err(Error::not_found(format!("no open window matches {spec}"))),
        1 => Ok(found.remove(0)),
        _ => Err(resolve::ambiguous(spec, found.iter(), "pass one window ref")),
    }
}

#[cfg(test)]
mod tests {
    use deckhand_core::WindowRef;

    use super::*;
    use crate::test_fixture::window;

    fn of_class(hwnd: i64, class_name: &str) -> WindowInfo {
        WindowInfo {
            class_name: class_name.to_string(),
            ..window(hwnd, "explorer.exe", "Program Manager", Rect::default())
        }
    }

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
        assert!(error.message.ends_with("; pass one window ref"), "{}", error.message);
        assert_eq!(single_window("Nothing", Vec::new()).unwrap_err().code, ErrorCode::NotFound);
    }

    #[test]
    fn shell_windows_are_refused_even_when_listed() {
        let desktop = of_class(1, "Progman");
        let error = require_app_window(&desktop, std::slice::from_ref(&desktop)).unwrap_err();
        assert_eq!(error.code, ErrorCode::Denied);
        assert!(error.message.starts_with("w1 (Progman) is part of the Windows shell"), "{}", error.message);
        for class_name in ["WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd"] {
            let shell = of_class(2, class_name);
            assert!(require_app_window(&shell, std::slice::from_ref(&shell)).is_err(), "{class_name}");
        }
    }

    #[test]
    fn a_window_must_be_one_deckhand_lists() {
        let app = of_class(3, "Notepad");
        assert!(require_app_window(&app, std::slice::from_ref(&app)).is_ok());
        assert_eq!(require_app_window(&app, &[of_class(4, "Notepad")]).unwrap_err().code, ErrorCode::NotFound);
    }

    #[test]
    fn state_changes_are_named() {
        assert_eq!(state_word(WindowState::Minimized), "minimized");
        assert_eq!(state_word(WindowState::Normal), "restored");
    }

    #[test]
    fn only_window_steps_become_window_requests() {
        let step: Step = serde_json::from_str(r#"{"do":"window","action":"close","target":"w1"}"#).unwrap();
        let request = WindowRequest::try_from(&step).unwrap();
        assert_eq!((request.action, request.app), (WindowAction::Close, None));
        let other: Step = serde_json::from_str(r#"{"do":"wait","ms":5}"#).unwrap();
        assert!(WindowRequest::try_from(&other).is_err());
    }
}
