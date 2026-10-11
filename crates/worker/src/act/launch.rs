use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use deckhand_core::{Error, ErrorCode, Result, WindowInfo, format::window_line};
use deckhand_desktop::apps::{self, AppEntry};

use super::{
    done::Done,
    launch_match::{AppIdentity, Before, WindowFacts, is_fresh, pick_launched, switched_to},
    settle,
};
use crate::{
    app_match::{self, without_launch_extension},
    call_scope::CallScope,
    engine::Engine,
    overlay,
    until::until,
    watch::Watch,
    window_classes::is_frame_host,
};

const WINDOW_WAIT: Duration = Duration::from_secs(20);
const FRESH_LISTING: Duration = Duration::from_secs(300);
const RECENT_LISTING: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
enum LaunchTarget {
    Path(PathBuf),
    App(AppEntry),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Started {
    label: String,
    pid: Option<u32>,
    identity: AppIdentity,
}

pub fn launch(engine: &Engine, app: &str, args: Option<&str>, call: &CallScope) -> Result<Done> {
    let target = launch_target(engine, app.trim())?;
    engine.guard.before_input(None, call)?;
    let before = before_launch()?;
    let started = start(&target, args)?;
    let found = until(engine, call, WINDOW_WAIT, &Watch::any(), || launched_window(&before, &started))?;
    let window = found.ok_or_else(|| {
        Error::timeout(format!(
            "launched {} but no window of it appeared within {} s",
            started.label,
            WINDOW_WAIT.as_secs()
        ))
    })?;
    engine.guard.check_window(&window)?;
    overlay::begin(engine, &window, call);
    settle(engine, &window, call);
    let window = deckhand_desktop::window(window.hwnd)?;
    Ok(Done::noting(format!("launched {}", window_line(&window))))
}

fn launch_target(engine: &Engine, app: &str) -> Result<LaunchTarget> {
    if !has_separator(app) {
        return find_app(engine, without_launch_extension(app)).map(LaunchTarget::App);
    }
    let path = engine.guard.checked_path(Path::new(app))?;
    if !path.exists() {
        return Err(Error::not_found(format!("{} does not exist", path.display())));
    }
    Ok(LaunchTarget::Path(path))
}

fn has_separator(app: &str) -> bool {
    app.contains(['\\', '/'])
}

fn find_app(engine: &Engine, name: &str) -> Result<AppEntry> {
    let apps = engine.apps.apps(FRESH_LISTING)?;
    match app_match::find(&apps, name) {
        Err(error) if error.code == ErrorCode::NotFound => {
            app_match::find(&engine.apps.apps(RECENT_LISTING)?, name).cloned()
        }
        result => result.cloned(),
    }
}

fn start(target: &LaunchTarget, args: Option<&str>) -> Result<Started> {
    match target {
        LaunchTarget::Path(path) => Ok(Started {
            label: path.display().to_string(),
            pid: apps::launch_path(path, args)?,
            identity: AppIdentity::of_path(path),
        }),
        LaunchTarget::App(app) => {
            Ok(Started { label: app.name.clone(), pid: apps::launch(app, args)?, identity: AppIdentity::of_app(app) })
        }
    }
}

fn before_launch() -> Result<Before> {
    let windows = deckhand_desktop::windows()?;
    Ok(Before {
        windows: windows.iter().map(|window| window.hwnd).collect(),
        foreground: deckhand_desktop::foreground_window(),
    })
}

fn launched_window(before: &Before, started: &Started) -> Result<Option<WindowInfo>> {
    let windows = deckhand_desktop::windows()?;
    let candidates: Vec<WindowFacts<'_>> = windows
        .iter()
        .filter(|window| is_fresh(window, before) || switched_to(window, before))
        .map(|window| facts_of(window, &started.identity))
        .collect();
    Ok(pick_launched(&candidates, before, &started.identity, started.pid).cloned())
}

fn facts_of<'a>(window: &'a WindowInfo, identity: &AppIdentity) -> WindowFacts<'a> {
    if !identity.needs_packages() {
        return WindowFacts { window, package: None, content_package: None };
    }
    let content = is_frame_host(window).then(|| deckhand_desktop::uwp_content_pid(window.hwnd)).flatten();
    WindowFacts {
        window,
        package: deckhand_desktop::package_family(window.pid),
        content_package: content.and_then(deckhand_desktop::package_family),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixture::engine;

    #[test]
    fn paths_are_told_apart_from_names_by_their_separators() {
        assert!(has_separator("C:/Tools/app.exe") && has_separator("tools\\app.exe") && !has_separator("app.exe"));
    }

    #[test]
    fn missing_and_relative_paths_fail_before_launching() {
        let missing = std::env::temp_dir().join("deckhand-no-such-app-7f3a.exe");
        let error = launch_target(engine(), &missing.to_string_lossy()).unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
        assert_eq!(launch_target(engine(), "tools\\app.exe").unwrap_err().code, ErrorCode::InvalidArgument);
    }

    #[test]
    fn unknown_names_fail_before_launching() {
        let error = launch_target(engine(), "zzqx-no-such-app-zzqx").unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
    }
}
