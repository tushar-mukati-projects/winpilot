use windows::Win32::{Foundation::HWND, UI::Input::KeyboardAndMouse::IsWindowEnabled};
use winpilot_core::{Rect, Result, WindowInfo, WindowRef};
use winpilot_win32::{
    class_name, foreground_hwnd, hwnd_exists, hwnd_of, is_visible, live_hwnd, owning_pid, ref_of, title,
    top_level_windows,
};

use crate::{
    process::ProcessLookup,
    virtual_desktops::{DesktopLookup, Placement},
    window_foreground::Foreground,
    window_geometry::visible_rect,
    window_visibility::{is_tool_window, show_state},
};

const DIALOG_CLASS: &str = "#32770";
const POPUP_MENU_CLASS: &str = "#32768";

struct Candidate {
    hwnd: HWND,
    pid: u32,
    title: String,
    class_name: String,
    rect: Rect,
    placement: Placement,
}

struct Lookups {
    foreground: Foreground,
    processes: ProcessLookup,
    desktops: DesktopLookup,
}

pub fn windows() -> Result<Vec<WindowInfo>> {
    let own_pid = std::process::id();
    let mut lookups = Lookups::new();
    let candidates: Vec<Candidate> =
        top_level_windows()?.into_iter().filter_map(|hwnd| app_window(hwnd, own_pid, &lookups.desktops)).collect();
    Ok(candidates.into_iter().map(|candidate| describe(candidate, &mut lookups)).collect())
}

pub fn window(hwnd: WindowRef) -> Result<WindowInfo> {
    let handle = live_hwnd(hwnd)?;
    let mut lookups = Lookups::new();
    let placement = match lookups.desktops.placement(handle) {
        Placement::Hidden => Placement::Current,
        found => found,
    };
    let candidate = Candidate {
        hwnd: handle,
        pid: owning_pid(handle),
        title: title(handle),
        class_name: class_name(handle),
        rect: visible_rect(handle).unwrap_or_default(),
        placement,
    };
    Ok(describe(candidate, &mut lookups))
}

impl Lookups {
    fn new() -> Self {
        Self { foreground: Foreground::now(), processes: ProcessLookup::new(), desktops: DesktopLookup::new() }
    }
}

pub fn is_enabled(hwnd: WindowRef) -> bool {
    let handle = hwnd_of(hwnd);
    hwnd_exists(handle) && unsafe { IsWindowEnabled(handle) }.as_bool()
}

pub fn foreground_window() -> Option<WindowRef> {
    foreground_hwnd().map(ref_of)
}

fn app_window(hwnd: HWND, own_pid: u32, desktops: &DesktopLookup) -> Option<Candidate> {
    if !is_visible(hwnd) {
        return None;
    }
    let class_name = class_name(hwnd);
    let popup_menu = class_name == POPUP_MENU_CLASS;
    if !popup_menu && is_tool_window(hwnd) {
        return None;
    }
    let pid = owning_pid(hwnd);
    if pid == own_pid {
        return None;
    }
    let title = title(hwnd);
    if title.is_empty() && !may_be_untitled(&class_name) {
        return None;
    }
    let rect = visible_rect(hwnd).filter(|rect| !rect.is_empty())?;
    let placement = desktops.placement(hwnd);
    if matches!(placement, Placement::Hidden | Placement::Unknown) {
        return None;
    }
    Some(Candidate { hwnd, pid, title, class_name, rect, placement })
}

fn may_be_untitled(class_name: &str) -> bool {
    matches!(class_name, DIALOG_CLASS | POPUP_MENU_CLASS)
}

fn describe(candidate: Candidate, lookups: &mut Lookups) -> WindowInfo {
    let facts = lookups.processes.facts(candidate.pid);
    let (desktop, on_current_desktop) = match candidate.placement {
        Placement::Elsewhere(number) => (number, false),
        Placement::Unknown => (None, false),
        Placement::Current | Placement::Hidden => (lookups.desktops.current_number(), true),
    };
    WindowInfo {
        hwnd: ref_of(candidate.hwnd),
        title: candidate.title,
        process: facts.name,
        pid: candidate.pid,
        class_name: candidate.class_name,
        rect: candidate.rect,
        state: show_state(candidate.hwnd),
        foreground: lookups.foreground.includes(candidate.hwnd),
        elevated: facts.elevated,
        desktop,
        on_current_desktop,
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use winpilot_core::Point;
    use winpilot_win32::{window_at, window_exists};

    use super::*;

    #[test]
    #[ignore = "reads the user's windows"]
    fn windows_have_visible_rects_and_exclude_this_process() {
        let list = windows().unwrap();
        assert!(!list.is_empty(), "no app windows found");
        assert!(list.iter().all(|info| !info.rect.is_empty()));
        assert!(list.iter().all(|info| info.pid != std::process::id()));
        assert!(list.iter().all(|info| !info.process.is_empty()));
    }

    #[test]
    #[ignore = "reads the user's windows"]
    fn listed_windows_say_where_they_are() {
        let list = windows().unwrap();
        assert!(list.iter().any(|info| info.on_current_desktop));
        assert!(list.iter().filter(|info| info.foreground).all(|info| info.on_current_desktop));
        let current = DesktopLookup::new().current_number();
        assert!(list.iter().filter(|info| info.on_current_desktop).all(|info| info.desktop == current));
    }

    #[test]
    #[ignore = "reads the user's windows"]
    fn the_window_at_a_point_is_a_live_top_level_window() {
        let Some(first) = windows().unwrap().into_iter().find(|info| info.on_current_desktop) else { return };
        if let Some(hit) = window_at(first.rect.center()) {
            assert!(window_exists(hit));
            assert_eq!(window_at(first.rect.center()), Some(hit));
        }
        assert_eq!(window_at(Point::new(-90_000, -90_000)), None);
    }

    #[test]
    #[ignore = "reads the user's windows"]
    fn foreground_window_exists() {
        assert!(foreground_window().is_some());
    }

    #[test]
    #[ignore = "reads the user's windows"]
    fn window_round_trips_the_first_window() {
        let list = windows().unwrap();
        let first = list.first().unwrap();
        let again = window(first.hwnd).unwrap();
        assert!(again.hwnd == first.hwnd, "hwnd differs");
        assert!(again.pid == first.pid, "pid differs");
        assert!(again.class_name == first.class_name, "class differs");
        assert!(again.process == first.process, "process differs");
        assert!(window_exists(first.hwnd));
        let _either_state_is_valid = is_enabled(first.hwnd);
    }

    #[test]
    fn missing_window_is_not_found() {
        let error = window(WindowRef(0)).unwrap_err();
        assert_eq!(error.code, winpilot_core::ErrorCode::NotFound);
        assert!(!window_exists(WindowRef(0)));
        assert!(!is_enabled(WindowRef(0)));
    }

    #[test]
    fn dialogs_and_popup_menus_may_be_untitled() {
        assert!(may_be_untitled("#32770"));
        assert!(may_be_untitled("#32768"));
        assert!(!may_be_untitled("tooltips_class32"));
        assert!(!may_be_untitled("Notepad"));
    }

    #[test]
    #[ignore = "reads the user's windows"]
    fn enumeration_is_fast() {
        let best = (0..5)
            .map(|_| {
                let started = Instant::now();
                let _list = windows().unwrap();
                started.elapsed()
            })
            .min()
            .unwrap();
        assert!(best < Duration::from_millis(20), "windows() took {best:?}");
        eprintln!("timing: windows() best of 5 took {:.3} ms", best.as_secs_f64() * 1000.0);
    }
}
