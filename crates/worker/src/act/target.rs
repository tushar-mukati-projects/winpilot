use deckhand_core::{Error, ErrorCode, Result, TargetSpec, WindowInfo};

use crate::{
    call_scope::CallScope,
    default_window::default_window,
    engine::Engine,
    overlay,
    resolve::{self, Reach, Resolved},
};

const HOST_TERMINAL: &str = "the foreground window is Claude's own terminal; give a target or focus the app first";
const FOREGROUND_MOVED: &str = "the foreground window changed while waiting for the user; give a target or try again";

pub type Prepared = Resolved;

pub fn prepare(engine: &Engine, spec: &TargetSpec, call: &CallScope) -> Result<Prepared> {
    let target = spec.parse()?;
    let resolved = resolve::target(engine, &target, call)?;
    ready(engine, resolved, call)
}

pub fn prepare_default_window(engine: &Engine, call: &CallScope) -> Result<Prepared> {
    ready(engine, Resolved { window: default_window(engine)?, reach: Reach::Window }, call)
}

pub fn foreground_app(engine: &Engine, call: &CallScope) -> Result<WindowInfo> {
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

fn ready(engine: &Engine, resolved: Resolved, call: &CallScope) -> Result<Prepared> {
    engine.host.refuse(&resolved.window)?;
    engine.guard.before_input(Some(&resolved.window), call)?;
    overlay::begin(engine, &resolved.window, call);
    Ok(resolved)
}

fn foreground_window() -> Result<WindowInfo> {
    let hwnd =
        deckhand_desktop::foreground_window().ok_or_else(|| Error::not_found("no window is in the foreground"))?;
    deckhand_desktop::window(hwnd)
}
