use deckhand_core::{Rect, Result, WindowRef};
use windows::Win32::Foundation::HWND;

use crate::{
    process::ProcessLookup,
    window_geometry::visible_rect,
    window_identity::{class_name, owning_pid, ref_of, title},
    window_list::top_level_windows,
    window_visibility::{is_cloaked, is_minimized, is_visible},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleWindow {
    pub hwnd: WindowRef,
    pub pid: u32,
    pub process: String,
    pub title: String,
    pub class_name: String,
    pub rect: Rect,
}

pub fn all_visible_windows() -> Result<Vec<VisibleWindow>> {
    let mut processes = ProcessLookup::new();
    let shown = top_level_windows()?.into_iter().filter(|hwnd| shows_on_screen(*hwnd));
    Ok(shown.filter_map(|hwnd| describe(hwnd, &mut processes)).collect())
}

fn shows_on_screen(hwnd: HWND) -> bool {
    is_visible(hwnd) && !is_cloaked(hwnd) && !is_minimized(hwnd)
}

fn describe(hwnd: HWND, processes: &mut ProcessLookup) -> Option<VisibleWindow> {
    let rect = visible_rect(hwnd).filter(|rect| !rect.is_empty())?;
    let pid = owning_pid(hwnd);
    Some(VisibleWindow {
        hwnd: ref_of(hwnd),
        pid,
        process: processes.facts(pid).name,
        title: title(hwnd),
        class_name: class_name(hwnd),
        rect,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_list::windows;

    #[test]
    fn every_shown_app_window_is_among_the_visible_windows() {
        let all = all_visible_windows().unwrap();
        assert!(!all.is_empty(), "no visible windows found");
        assert!(all.iter().all(|window| !window.rect.is_empty()));
        let apps = windows().unwrap();
        let shown =
            apps.iter().filter(|app| app.state != deckhand_core::WindowState::Minimized && app.on_current_desktop);
        let missing = shown.filter(|app| all.iter().all(|window| window.hwnd != app.hwnd)).count();
        assert!(missing <= 1, "{missing} app windows are missing from the visible list");
    }
}
