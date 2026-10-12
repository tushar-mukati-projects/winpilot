use std::borrow::Cow;

use deckhand_core::{Error, ErrorCode, Result, TargetSpec, WindowInfo};

use super::mode::Mode;
use crate::{
    call_scope::CallScope,
    default_window::default_window,
    engine::Engine,
    host_terminal, overlay,
    resolve::{self, Cover, Reach, Resolved},
};

const HOST_TERMINAL: &str = "the foreground window is the agent's own terminal; give a target or focus the app first";
const FOREGROUND_MOVED: &str = "the foreground window changed while waiting for the user; give a target or try again";

pub(crate) type Prepared = Resolved;

pub(crate) fn prepare(engine: &Engine, spec: &TargetSpec, mode: Mode, call: &CallScope) -> Result<Prepared> {
    let target = spec.parse()?;
    let resolved = resolve::target_with(engine, &target, call, cover_for(mode))?;
    ready(engine, resolved, mode, call)
}

const fn cover_for(mode: Mode) -> Cover {
    match mode {
        Mode::Foreground => Cover::Refused,
        Mode::Background => Cover::Ignored,
    }
}

pub(crate) fn prepare_default_window(engine: &Engine, call: &CallScope) -> Result<Prepared> {
    ready(engine, Resolved { window: default_window(engine)?, reach: Reach::Window }, Mode::Foreground, call)
}

pub(crate) fn given_or_else(
    target: Option<&Prepared>,
    fallback: impl FnOnce() -> Result<Prepared>,
) -> Result<Cow<'_, Prepared>> {
    match target {
        Some(prepared) => Ok(Cow::Borrowed(prepared)),
        None => fallback().map(Cow::Owned),
    }
}

pub(crate) fn foreground_app(engine: &Engine, call: &CallScope) -> Result<WindowInfo> {
    let window = foreground_window()?;
    if engine.host.contains(window.pid) {
        return Err(Error::new(ErrorCode::Denied, HOST_TERMINAL));
    }
    engine.guard.before_input(Some(&window), call)?;
    if foreground_window()?.hwnd != window.hwnd {
        return Err(Error::new(ErrorCode::UserActive, FOREGROUND_MOVED));
    }
    overlay::begin(engine, &window, call);
    Ok(window)
}

fn ready(engine: &Engine, resolved: Resolved, mode: Mode, call: &CallScope) -> Result<Prepared> {
    host_terminal::check_target(engine, &resolved)?;
    if mode == Mode::Foreground && resolved.window.on_current_desktop {
        engine.guard.before_input(Some(&resolved.window), call)?;
    } else {
        engine.guard.before_patterns(Some(&resolved.window), call)?;
    }
    overlay::begin(engine, &resolved.window, call);
    Ok(resolved)
}

fn foreground_window() -> Result<WindowInfo> {
    let hwnd =
        deckhand_desktop::foreground_window().ok_or_else(|| Error::not_found("no window is in the foreground"))?;
    deckhand_desktop::window(hwnd)
}

#[cfg(test)]
mod tests {
    use deckhand_core::{Rect, WindowRef};

    use super::*;
    use crate::test_fixture::window;

    fn whole(hwnd: i64) -> Prepared {
        Resolved { window: window(hwnd, "app.exe", "App", Rect::new(0, 0, 10, 10)), reach: Reach::Window }
    }

    #[test]
    fn a_given_target_is_borrowed_and_only_a_missing_one_runs_the_fallback() {
        let given = whole(1);
        let chosen = given_or_else(Some(&given), || Err(Error::internal("the fallback ran"))).unwrap();
        assert!(matches!(chosen, Cow::Borrowed(_)));
        assert_eq!(given_or_else(None, || Ok(whole(2))).unwrap().window.hwnd, WindowRef(2));
        assert!(given_or_else(None, || Err(Error::not_found("nothing here"))).is_err());
    }

    #[test]
    fn only_background_steps_reach_a_screenshot_point_behind_another_window() {
        assert_eq!(cover_for(Mode::Foreground), Cover::Refused);
        assert_eq!(cover_for(Mode::Background), Cover::Ignored);
    }
}
