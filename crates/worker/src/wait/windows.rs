use std::collections::HashSet;

use deckhand_core::{Result, WindowInfo, WindowRef, format::window_line};

use super::check::{Check, unless_gone};
use crate::{app_windows::app_windows, engine::Engine, resolve};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct WindowsBefore(HashSet<WindowRef>);

impl WindowsBefore {
    pub(crate) fn now(engine: &Engine) -> Self {
        let windows = app_windows(engine).unwrap_or_default();
        Self(windows.iter().map(|window| window.hwnd).collect())
    }

    pub(crate) fn contains(&self, hwnd: WindowRef) -> bool {
        self.0.contains(&hwnd)
    }
}

pub(crate) fn open(engine: &Engine, title: &str) -> Result<Check> {
    matching(engine, title).map(|found| window_open(title, &found))
}

pub(crate) fn opens(engine: &Engine, title: &str, before: &WindowsBefore) -> Result<Check> {
    matching(engine, title).map(|found| window_opens(title, &found, before))
}

pub(crate) fn closes(engine: &Engine, title: &str) -> Result<Check> {
    matching(engine, title).map(|found| window_closes(title, &found))
}

fn window_open(title: &str, found: &[WindowInfo]) -> Check {
    match found.first() {
        Some(window) => Check::met(format!("{} is open", window_line(window))),
        None => Check::unmet(format!("no window matches \"{title}\"")),
    }
}

fn window_opens(title: &str, found: &[WindowInfo], before: &WindowsBefore) -> Check {
    let (fresh, already): (Vec<&WindowInfo>, Vec<&WindowInfo>) =
        found.iter().partition(|window| !before.contains(window.hwnd));
    match (fresh.first(), already.len()) {
        (Some(window), _) => Check::met(format!("{} is open", window_line(window))),
        (None, 0) => Check::unmet(format!("no window matches \"{title}\"")),
        (None, open) => {
            Check::unmet(format!("no new window matches \"{title}\" ({open} was already open before the wait)"))
        }
    }
}

fn window_closes(title: &str, found: &[WindowInfo]) -> Check {
    match found.first() {
        Some(window) => Check::unmet(format!("{} is still open", window_line(window))),
        None => Check::met(format!("no window matches \"{title}\"")),
    }
}

fn matching(engine: &Engine, title: &str) -> Result<Vec<WindowInfo>> {
    Ok(unless_gone(resolve::windows_matching(engine, title))?.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use deckhand_core::Rect;

    use super::*;

    fn dialog(hwnd: i64) -> WindowInfo {
        let window = crate::test_fixture::window(hwnd, "notepad.exe", "Save As", Rect::new(0, 0, 300, 200));
        WindowInfo { pid: 7, foreground: true, ..window }
    }

    fn before(hwnds: &[i64]) -> WindowsBefore {
        WindowsBefore(hwnds.iter().copied().map(WindowRef).collect())
    }

    #[test]
    fn a_newly_opened_window_satisfies_opens_and_blocks_closes() {
        let opened = window_opens("Save As", &[dialog(42)], &before(&[]));
        assert!(opened.met);
        assert_eq!(opened.detail, "w42 notepad.exe \"Save As\" foreground is open");
        let still = window_closes("Save As", &[dialog(42)]);
        assert!(!still.met);
        assert_eq!(still.detail, "w42 notepad.exe \"Save As\" foreground is still open");
    }

    #[test]
    fn windows_open_before_the_wait_do_not_count() {
        let old = window_opens("Save As", &[dialog(42)], &before(&[42]));
        assert!(!old.met);
        assert_eq!(old.detail, "no new window matches \"Save As\" (1 was already open before the wait)");
        assert!(window_opens("Save As", &[dialog(42), dialog(43)], &before(&[42])).met);
    }

    #[test]
    fn a_window_already_open_satisfies_open_at_once() {
        let open = window_open("Save As", &[dialog(42)]);
        assert!(open.met);
        assert_eq!(open.detail, "w42 notepad.exe \"Save As\" foreground is open");
        let missing = window_open("Save As", &[]);
        assert!(!missing.met);
        assert_eq!(missing.detail, "no window matches \"Save As\"");
    }

    #[test]
    fn no_match_satisfies_closes_only() {
        assert!(!window_opens("Save As", &[], &before(&[])).met);
        let closed = window_closes("Save As", &[]);
        assert!(closed.met);
        assert_eq!(closed.detail, "no window matches \"Save As\"");
    }
}
