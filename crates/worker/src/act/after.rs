use std::collections::HashSet;

use winpilot_core::{
    Result, Snapshot, WindowInfo, WindowRef,
    diff::diff,
    protocol::{After, AfterMode, DiffReport},
};

use crate::{app_windows::app_windows, default_window::default_window, engine::Engine, observe};

pub(crate) struct Baseline {
    window: WindowInfo,
    snapshot: Snapshot,
    windows: Vec<WindowInfo>,
}

impl Baseline {
    pub(crate) fn capture(engine: &Engine, mode: AfterMode) -> Option<Self> {
        if mode != AfterMode::Diff {
            return None;
        }
        let window = default_window(engine).ok()?;
        Self::of(engine, window).ok()
    }

    pub(crate) fn of(engine: &Engine, window: WindowInfo) -> Result<Self> {
        let snapshot = engine.snapshot(&window)?;
        let windows = app_windows(engine)?;
        Ok(Self { window, snapshot, windows })
    }

    pub(crate) fn compare(&self, engine: &Engine, current: WindowInfo) -> Result<DiffReport> {
        let after = engine.snapshot(&current)?;
        let (opened, closed) = window_changes(&self.windows, &app_windows(engine)?);
        let focused = after.focused.and_then(|id| after.get(id)).cloned();
        Ok(DiffReport { window: current, focused, changes: diff(&self.snapshot, &after), opened, closed })
    }
}

pub(crate) fn report(engine: &Engine, mode: AfterMode, baseline: Option<&Baseline>) -> Result<After> {
    match mode {
        AfterMode::None => Ok(After::None),
        AfterMode::Screenshot => observe::of_window(engine, &default_window(engine)?).map(After::Screenshot),
        AfterMode::Observe => observe::observe_window(engine, &default_window(engine)?).map(After::Observation),
        AfterMode::Diff => diff_or_observe(engine, baseline),
    }
}

fn diff_or_observe(engine: &Engine, baseline: Option<&Baseline>) -> Result<After> {
    let current = default_window(engine)?;
    match baseline {
        Some(baseline) if baseline.window.hwnd == current.hwnd => baseline.compare(engine, current).map(After::Diff),
        _ => observe::observe_window(engine, &current).map(After::Observation),
    }
}

fn window_changes(before: &[WindowInfo], after: &[WindowInfo]) -> (Vec<WindowInfo>, Vec<WindowInfo>) {
    let earlier: HashSet<WindowRef> = before.iter().map(|window| window.hwnd).collect();
    let later: HashSet<WindowRef> = after.iter().map(|window| window.hwnd).collect();
    let opened = after.iter().filter(|window| !earlier.contains(&window.hwnd)).cloned().collect();
    let closed = before.iter().filter(|window| !later.contains(&window.hwnd)).cloned().collect();
    (opened, closed)
}

#[cfg(test)]
mod tests {
    use winpilot_core::Rect;

    use super::*;

    fn window(hwnd: i64, title: &str) -> WindowInfo {
        crate::test_fixture::window(hwnd, "app.exe", title, Rect::new(0, 0, 100, 100))
    }

    fn hwnds(windows: &[WindowInfo]) -> Vec<i64> {
        windows.iter().map(|window| window.hwnd.0).collect()
    }

    #[test]
    fn unchanged_windows_report_nothing() {
        let windows = [window(1, "A"), window(2, "B")];
        let (opened, closed) = window_changes(&windows, &windows);
        assert!(opened.is_empty() && closed.is_empty());
    }

    #[test]
    fn new_and_vanished_windows_are_found_by_handle() {
        let before = [window(1, "Main"), window(2, "Find")];
        let after = [window(3, "Save As"), window(1, "Main - edited")];
        let (opened, closed) = window_changes(&before, &after);
        assert_eq!(hwnds(&opened), [3]);
        assert_eq!(hwnds(&closed), [2]);
    }

    #[test]
    fn retitled_windows_are_neither_opened_nor_closed() {
        let (opened, closed) = window_changes(&[window(1, "Untitled")], &[window(1, "Saved")]);
        assert!(opened.is_empty() && closed.is_empty());
    }

    #[test]
    fn empty_lists() {
        let (opened, closed) = window_changes(&[], &[window(5, "New")]);
        assert_eq!(hwnds(&opened), [5]);
        assert!(closed.is_empty());
        let (opened, closed) = window_changes(&[window(5, "Old")], &[]);
        assert!(opened.is_empty());
        assert_eq!(hwnds(&closed), [5]);
    }
}
