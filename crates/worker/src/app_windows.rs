use deckhand_core::{Result, WindowInfo, selector::Selector};

use crate::engine::Engine;

pub(crate) fn app_windows(engine: &Engine) -> Result<Vec<WindowInfo>> {
    let windows = deckhand_desktop::windows()?;
    Ok(windows.into_iter().filter(|window| !engine.guard.denies(window)).collect())
}

pub(crate) fn app_windows_matching(engine: &Engine, selector: &Selector) -> Result<Vec<WindowInfo>> {
    Ok(app_windows(engine)?.into_iter().filter(|window| selector.matches_window(window)).collect())
}
