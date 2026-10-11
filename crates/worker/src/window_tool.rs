mod app_catalog;
mod app_match;
mod launch;
mod launch_match;

use std::time::Duration;

use deckhand_core::{
    Error, ErrorCode, Result, WindowInfo, WindowRef, WindowState,
    format::window_line,
    protocol::{Observation, WindowAction, WindowsArgs, WindowsReport},
};

pub use app_catalog::AppCatalog;

use crate::{
    act, app_windows::app_windows, call_scope::CallScope, default_window::default_window, engine::Engine, observe,
    resolve, until::until, watch::Watch,
};

const CLOSE_WAIT: Duration = Duration::from_secs(3);
const STILL_OPEN: &str = "the window is still open; it may be asking something";
const LISTED_MATCHES: usize = 8;

pub fn run(engine: &Engine, args: &WindowsArgs, call: &CallScope) -> Result<WindowsReport> {
    args.validate()?;
    match args.action {
        WindowAction::List => list(engine),
        WindowAction::Focus => focus(engine, args, call),
        WindowAction::Launch => launch::launch(engine, args, call),
        WindowAction::Close => close(engine, args, call),
        WindowAction::Minimize => change_state(engine, args, call, WindowState::Minimized),
        WindowAction::Maximize => change_state(engine, args, call, WindowState::Maximized),
        WindowAction::Restore => change_state(engine, args, call, WindowState::Normal),
        WindowAction::Move => move_to(engine, args, call),
    }
}

fn list(engine: &Engine) -> Result<WindowsReport> {
    Ok(WindowsReport { windows: app_windows(engine)?, window: None, observation: None, note: None })
}

fn focus(engine: &Engine, args: &WindowsArgs, call: &CallScope) -> Result<WindowsReport> {
    controlled(engine, args, "focus", call, |window| {
        deckhand_desktop::focus_window(window.hwnd)?;
        act::settle(engine, window, call);
        let window = deckhand_desktop::window(window.hwnd)?;
        let observation = observe::observe_window(engine, &window)?;
        Ok(noted(window, Some(observation), "focused"))
    })
}

fn close(engine: &Engine, args: &WindowsArgs, call: &CallScope) -> Result<WindowsReport> {
    controlled(engine, args, "close", call, |window| {
        deckhand_desktop::close_window(window.hwnd)?;
        let hwnd = window.hwnd;
        let closed = until(engine, call, CLOSE_WAIT, &Watch::window(window), || {
            Ok((!deckhand_desktop::is_window(hwnd)).then_some(()))
        })?;
        if closed.is_some() {
            return Ok(noted(window.clone(), None, "closed"));
        }
        let observation = prompt_window(engine, hwnd).and_then(|prompt| observe::observe_window(engine, &prompt)).ok();
        Ok(noted(window.clone(), observation, STILL_OPEN))
    })
}

fn prompt_window(engine: &Engine, owner: WindowRef) -> Result<WindowInfo> {
    let asking = app_windows(engine)?.into_iter().find(|window| deckhand_desktop::owner_of(window.hwnd) == Some(owner));
    asking.map_or_else(|| default_window(engine), Ok)
}

fn change_state(engine: &Engine, args: &WindowsArgs, call: &CallScope, state: WindowState) -> Result<WindowsReport> {
    let (action, note) = state_words(state);
    controlled(engine, args, action, call, |window| {
        deckhand_desktop::set_window_state(window.hwnd, state)?;
        act::settle(engine, window, call);
        Ok(noted(deckhand_desktop::window(window.hwnd)?, None, note))
    })
}

const fn state_words(state: WindowState) -> (&'static str, &'static str) {
    match state {
        WindowState::Minimized => ("minimize", "minimized"),
        WindowState::Maximized => ("maximize", "maximized"),
        WindowState::Normal => ("restore", "restored"),
    }
}

fn move_to(engine: &Engine, args: &WindowsArgs, call: &CallScope) -> Result<WindowsReport> {
    let rect = args.rect.ok_or_else(|| Error::invalid("move needs \"rect\""))?;
    controlled(engine, args, "move", call, |window| {
        deckhand_desktop::move_window(window.hwnd, rect)?;
        act::settle(engine, window, call);
        Ok(noted(deckhand_desktop::window(window.hwnd)?, None, "moved"))
    })
}

fn controlled<T>(
    engine: &Engine,
    args: &WindowsArgs,
    action: &str,
    call: &CallScope,
    perform: impl FnOnce(&WindowInfo) -> Result<T>,
) -> Result<T> {
    let window = unique_window(engine, args, action)?;
    let _actor = engine.guard.acquire_actor(call)?;
    engine.guard.before_input(Some(&window), call)?;
    perform(&window)
}

fn unique_window(engine: &Engine, args: &WindowsArgs, action: &str) -> Result<WindowInfo> {
    let spec = args.window.as_deref().map(str::trim).filter(|spec| !spec.is_empty());
    let spec = spec.ok_or_else(|| Error::invalid(format!("{action} needs \"window\"")))?;
    let window = single_window(spec, resolve::windows_matching(engine, spec)?)?;
    engine.guard.check_window(&window)?;
    engine.host.refuse(&window)?;
    Ok(window)
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

fn noted(window: WindowInfo, observation: Option<Observation>, note: &str) -> WindowsReport {
    WindowsReport { windows: Vec::new(), window: Some(window), observation, note: Some(note.to_string()) }
}

#[cfg(test)]
mod tests {
    use deckhand_core::{Rect, format};

    use super::*;
    use crate::test_fixture::{ClosesTitled, FixtureApp, engine, fixture_exe, unique_title, window};

    fn args(action: WindowAction) -> WindowsArgs {
        WindowsArgs { action, window: None, app: None, args: None, rect: None }
    }

    fn on(action: WindowAction, app: &FixtureApp) -> WindowsArgs {
        WindowsArgs { window: Some(app.hwnd().to_string()), ..args(action) }
    }

    fn act_on(wanted: &WindowsArgs) -> Result<WindowsReport> {
        run(engine(), wanted, &CallScope::relaxed())
    }

    fn invalid(wanted: &WindowsArgs) -> String {
        let error = act_on(wanted).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        error.message
    }

    #[test]
    fn list_shows_allowed_app_windows() {
        let report = act_on(&args(WindowAction::List)).unwrap();
        assert!(!report.windows.is_empty());
        assert!(report.windows.iter().all(|window| !engine().guard.denies(window)));
        assert!(report.window.is_none() && report.note.is_none());
        assert_eq!(format::windows(&report).lines().count(), report.windows.len());
    }

    #[test]
    fn missing_fields_are_named() {
        for action in [WindowAction::Focus, WindowAction::Close, WindowAction::Minimize, WindowAction::Restore] {
            assert!(invalid(&args(action)).contains("needs \"window\""));
        }
        assert_eq!(invalid(&args(WindowAction::Maximize)), "maximize needs \"window\"");
        assert_eq!(invalid(&args(WindowAction::Move)), "move needs \"rect\"");
        let empty = WindowsArgs {
            rect: Some(Rect::new(0, 0, 0, 10)),
            window: Some("w1".to_string()),
            ..args(WindowAction::Move)
        };
        assert!(invalid(&empty).contains("positive size"));
        assert_eq!(invalid(&args(WindowAction::Launch)), "launch needs \"app\"");
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
    #[ignore = "opens dialogs / changes focus"]
    fn focus_brings_the_fixture_forward() {
        let Some(app) = FixtureApp::launch() else { return };
        let report = act_on(&on(WindowAction::Focus, &app)).unwrap();
        assert!(report.window.unwrap().foreground);
        assert!(report.observation.is_some());
    }

    #[test]
    #[ignore = "opens dialogs / changes focus"]
    fn minimize_and_restore_the_fixture() {
        let Some(app) = FixtureApp::launch() else { return };
        let minimized = act_on(&on(WindowAction::Minimize, &app)).unwrap();
        assert_eq!(minimized.window.unwrap().state, WindowState::Minimized);
        let restored = act_on(&on(WindowAction::Restore, &app)).unwrap();
        assert_eq!(restored.window.unwrap().state, WindowState::Normal);
    }

    #[test]
    #[ignore = "opens dialogs / changes focus"]
    fn move_the_fixture() {
        let Some(app) = FixtureApp::launch() else { return };
        let target = Rect::new(app.window.rect.x + 40, app.window.rect.y + 30, 500, 400);
        let report = act_on(&WindowsArgs { rect: Some(target), ..on(WindowAction::Move, &app) });
        assert_eq!(report.unwrap().window.unwrap().rect, target);
    }

    #[test]
    #[ignore = "opens dialogs / changes focus"]
    fn close_the_fixture() {
        let Some(app) = FixtureApp::launch() else { return };
        let report = act_on(&on(WindowAction::Close, &app)).unwrap();
        assert_eq!(report.note.as_deref(), Some("closed"));
        assert!(!deckhand_desktop::is_window(app.hwnd()));
    }

    #[test]
    #[ignore = "opens dialogs / changes focus"]
    fn launch_the_fixture_by_path() {
        let Some(exe) = fixture_exe() else { return };
        let exe = std::fs::canonicalize(exe).unwrap();
        let title = unique_title("Launch");
        let _close = ClosesTitled(title.clone());
        let wanted = WindowsArgs {
            app: Some(exe.to_string_lossy().trim_start_matches("\\\\?\\").to_string()),
            args: Some(format!("--title \"{title}\"")),
            ..args(WindowAction::Launch)
        };
        let report = act_on(&wanted).unwrap();
        let window = report.window.unwrap();
        assert_eq!(window.title, title);
        assert_eq!(report.note.as_deref(), Some("launched"));
        assert!(report.observation.is_some());
    }
}
