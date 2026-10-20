use std::{
    process,
    time::{Duration, Instant},
};

use winpilot_core::{Error, ErrorCode, Result, Snapshot, WindowInfo, WindowRef, selector::Selector};
use winpilot_win32::{is_visible, owner_of, owning_pid, ref_of, top_level_windows};

use super::{Reach, Resolved, ambiguous, resolve_window, searched};
use crate::{
    app_windows::app_windows_matching,
    call_scope::CallScope,
    default_window::default_window,
    engine::Engine,
    ocr_read::Freshness,
    ocr_target::{self, OcrUse},
    until::until,
    wait::unless_gone,
    watch::Watch,
};

const NARROW_THE_SELECTOR: &str =
    "add window: or process: terms to say which, or observe that window and use its element ref";

trait InWindow {
    fn window(&self) -> &WindowInfo;
}

impl InWindow for WindowInfo {
    fn window(&self) -> &WindowInfo {
        self
    }
}

impl InWindow for Resolved {
    fn window(&self) -> &WindowInfo {
        &self.window
    }
}

pub(super) fn wait_for_selector(engine: &Engine, selector: &Selector, call: &CallScope) -> Result<Resolved> {
    let patience = Duration::from_millis(engine.config.act.auto_wait_ms);
    let mut missing = None;
    let mut freshness = Freshness::CachedIsFine;
    let found = until(engine, call, patience, &selector_watch(engine, selector), || {
        let using =
            OcrUse { call, freshness: std::mem::replace(&mut freshness, Freshness::TakenAfter(Instant::now())) };
        match resolve_selector(engine, selector, using) {
            Ok(resolved) => Ok(Some(resolved)),
            Err(error) if error.code == ErrorCode::NotFound => {
                missing = Some(error);
                Ok(None)
            }
            Err(error) => Err(error),
        }
    })?;
    found.ok_or_else(|| missing.unwrap_or_else(|| no_window(selector)))
}

fn selector_watch(engine: &Engine, selector: &Selector) -> Watch {
    candidate_windows(engine, selector).map(|windows| Watch::windows(&windows)).unwrap_or_default()
}

fn candidate_windows(engine: &Engine, selector: &Selector) -> Result<Vec<WindowInfo>> {
    if selector.has_window_terms() {
        return app_windows_matching(engine, selector);
    }
    default_window(engine).map(|window| vec![window])
}

fn resolve_selector(engine: &Engine, selector: &Selector, using: OcrUse<'_>) -> Result<Resolved> {
    let candidates = candidate_windows(engine, selector)?;
    if let Some(text) = selector.ocr_text() {
        let window = one_preferred(candidates, selector)?;
        return ocr_target::resolve(engine, window, text, selector.nth(), using);
    }
    if !selector.has_element_terms() {
        let window = one_preferred(candidates, selector)?;
        return resolve_window(engine, window);
    }
    let here = candidates.iter().map(|window| engine.snapshot(window)).collect::<Result<_>>()?;
    let snapshots = selector_snapshots(engine, selector, here, using.call.deadline())?;
    one_preferred(element_matches(selector, snapshots)?, selector)
}

pub(crate) fn selector_snapshots(
    engine: &Engine,
    selector: &Selector,
    here: Vec<Snapshot>,
    ends: Instant,
) -> Result<Vec<Snapshot>> {
    let owner = owner_to_search(selector, here.first());
    let mut snapshots: Vec<Snapshot> =
        here.into_iter().map(|snapshot| searched(engine, snapshot, selector, ends)).collect::<Result<_>>()?;
    if let Some(owner) = owner
        && snapshots.iter().all(|snapshot| selector.find(snapshot).is_empty())
    {
        for window in owned_windows(engine, owner)? {
            if let Some(snapshot) = readable(engine.snapshot(&window))? {
                snapshots.push(searched(engine, snapshot, selector, ends)?);
            }
        }
    }
    Ok(snapshots)
}

fn owner_to_search(selector: &Selector, first: Option<&Snapshot>) -> Option<WindowRef> {
    first.map(|snapshot| snapshot.window.hwnd).filter(|_| !selector.has_window_terms())
}

fn owned_windows(engine: &Engine, owner: WindowRef) -> Result<Vec<WindowInfo>> {
    let shown =
        top_level_windows()?.into_iter().filter(|hwnd| owner_of(ref_of(*hwnd)) == Some(owner) && is_visible(*hwnd));
    let mut windows = Vec::new();
    for hwnd in shown {
        let pid = owning_pid(hwnd);
        if pid == process::id() || engine.host.contains(pid) {
            continue;
        }
        if let Some(window) = unless_gone(winpilot_desktop::window(ref_of(hwnd)))?
            && !engine.guard.denies(&window)
        {
            windows.push(window);
        }
    }
    Ok(windows)
}

fn readable(snapshot: Result<Snapshot>) -> Result<Option<Snapshot>> {
    match unless_gone(snapshot) {
        Err(error) if matches!(error.code, ErrorCode::ElevatedTarget | ErrorCode::TargetNotResponding) => Ok(None),
        other => other,
    }
}

fn element_matches(selector: &Selector, snapshots: Vec<Snapshot>) -> Result<Vec<Resolved>> {
    let mut found = Vec::new();
    let mut last_error = no_window(selector);
    for snapshot in snapshots {
        match selector.resolve(&snapshot) {
            Ok(element) => found.push(Resolved { reach: Reach::Element(element.clone()), window: snapshot.window }),
            Err(error) if error.code == ErrorCode::NotFound => last_error = error,
            Err(error) => return Err(error),
        }
    }
    if found.is_empty() {
        return Err(last_error);
    }
    Ok(found)
}

fn one_preferred<T: InWindow>(matches: Vec<T>, selector: &Selector) -> Result<T> {
    let mut tied = preferred(matches);
    match tied.len() {
        0 => Err(no_window(selector)),
        1 => Ok(tied.remove(0)),
        _ => Err(ambiguous(&selector.to_string(), tied.iter().map(InWindow::window), NARROW_THE_SELECTOR)),
    }
}

fn preferred<T: InWindow>(matches: Vec<T>) -> Vec<T> {
    let here = kept_if_any(matches, |found| found.window().on_current_desktop);
    kept_if_any(here, |found| found.window().foreground)
}

fn kept_if_any<T>(items: Vec<T>, wanted: impl Fn(&T) -> bool) -> Vec<T> {
    if !items.iter().any(&wanted) {
        return items;
    }
    items.into_iter().filter(|item| wanted(item)).collect()
}

fn no_window(selector: &Selector) -> Error {
    Error::not_found(format!("no open window matches {selector}"))
}

#[cfg(test)]
mod tests {
    use winpilot_core::Rect;

    use super::*;
    use crate::test_fixture::{snapshot, window};

    fn placed(hwnd: i64, here: bool, foreground: bool) -> WindowInfo {
        let base = window(hwnd, "app.exe", "App", Rect::new(0, 0, 10, 10));
        WindowInfo { on_current_desktop: here, desktop: (!here).then_some(2), foreground, ..base }
    }

    fn selector(text: &str) -> Selector {
        text.parse().unwrap()
    }

    fn chosen(windows: Vec<WindowInfo>) -> Result<i64> {
        one_preferred(windows, &selector("process:app")).map(|window| window.hwnd.0)
    }

    #[test]
    fn a_selector_without_window_terms_also_searches_the_default_windows_owned_windows() {
        let default = Snapshot { window: placed(7, true, true), ..snapshot("App", Vec::new()) };
        assert_eq!(owner_to_search(&selector("role:button name:Save"), Some(&default)), Some(WindowRef(7)));
        assert_eq!(owner_to_search(&selector("window:Chrome >> role:button name:Save"), Some(&default)), None);
        assert_eq!(owner_to_search(&selector("process:chrome role:button"), Some(&default)), None);
        assert_eq!(owner_to_search(&selector("role:button"), None), None);
    }

    #[test]
    fn an_owned_window_that_cannot_be_read_is_skipped_but_other_failures_stop_the_search() {
        for code in
            [ErrorCode::NotFound, ErrorCode::StaleRef, ErrorCode::ElevatedTarget, ErrorCode::TargetNotResponding]
        {
            assert_eq!(readable(Err(Error::new(code, "x"))).unwrap(), None, "{code}");
        }
        for code in [ErrorCode::Internal, ErrorCode::Cancelled, ErrorCode::StoppedByUser, ErrorCode::Denied] {
            assert_eq!(readable(Err(Error::new(code, "x"))).unwrap_err().code, code);
        }
        let shown = snapshot("Menu", Vec::new());
        assert_eq!(readable(Ok(shown.clone())).unwrap(), Some(shown));
    }

    #[test]
    fn a_single_match_wins_wherever_it_is() {
        assert_eq!(chosen(vec![placed(1, false, false)]).unwrap(), 1);
        assert_eq!(chosen(Vec::new()).unwrap_err().code, ErrorCode::NotFound);
    }

    #[test]
    fn the_current_desktop_then_the_foreground_break_ties() {
        assert_eq!(chosen(vec![placed(1, false, false), placed(2, true, false)]).unwrap(), 2);
        assert_eq!(chosen(vec![placed(1, true, false), placed(2, true, true)]).unwrap(), 2);
        assert_eq!(chosen(vec![placed(1, true, true), placed(2, false, true)]).unwrap(), 1);
    }

    #[test]
    fn matches_that_nothing_tells_apart_are_ambiguous_and_listed_topmost_first() {
        let error = chosen(vec![placed(1, false, false), placed(3, true, false), placed(2, true, false)]).unwrap_err();
        assert_eq!(error.code, ErrorCode::Ambiguous);
        assert!(error.message.starts_with("2 windows match process:app: w3 app.exe"), "{}", error.message);
        assert!(error.message.contains("; w2 app.exe"), "{}", error.message);
        assert!(!error.message.contains("w1 "), "{}", error.message);
        assert!(error.message.ends_with(NARROW_THE_SELECTOR), "{}", error.message);
    }
}
