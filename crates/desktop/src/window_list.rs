use deckhand_core::{Context, Rect, Result, WindowInfo, WindowRef};
use windows::{
    Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::{EnumWindows, GW_OWNER, GetWindow},
    },
    core::BOOL,
};

use crate::{
    process::ProcessLookup,
    window_facts::{
        Foreground, class_name, current_foreground, hwnd_of, is_cloaked, is_live, is_tool_window, is_visible,
        live_hwnd, owning_pid, ref_of, show_state, title, visible_rect,
    },
};

const DIALOG_CLASS: &str = "#32770";
const POPUP_MENU_CLASS: &str = "#32768";
const EXPECTED_TOP_LEVEL_WINDOWS: usize = 512;

struct Candidate {
    hwnd: HWND,
    pid: u32,
    title: String,
    class_name: String,
    rect: Rect,
}

pub fn windows() -> Result<Vec<WindowInfo>> {
    let own_pid = std::process::id();
    let foreground = Foreground::now();
    let mut processes = ProcessLookup::new();
    let infos = top_level_windows()?
        .into_iter()
        .filter_map(|hwnd| app_window(hwnd, own_pid))
        .map(|candidate| describe(candidate, foreground, &mut processes))
        .collect();
    Ok(infos)
}

pub fn window(hwnd: WindowRef) -> Result<WindowInfo> {
    let handle = live_hwnd(hwnd)?;
    let candidate = Candidate {
        hwnd: handle,
        pid: owning_pid(handle),
        title: title(handle),
        class_name: class_name(handle),
        rect: visible_rect(handle).unwrap_or_default(),
    };
    Ok(describe(candidate, Foreground::now(), &mut ProcessLookup::new()))
}

pub fn is_window(hwnd: WindowRef) -> bool {
    is_live(hwnd_of(hwnd))
}

pub fn foreground_window() -> Option<WindowRef> {
    current_foreground().map(ref_of)
}

pub fn owner_of(hwnd: WindowRef) -> Option<WindowRef> {
    let owner = unsafe { GetWindow(hwnd_of(hwnd), GW_OWNER) }.ok()?;
    (!owner.is_invalid()).then(|| ref_of(owner))
}

fn top_level_windows() -> Result<Vec<HWND>> {
    let mut handles: Vec<HWND> = Vec::with_capacity(EXPECTED_TOP_LEVEL_WINDOWS);
    unsafe { EnumWindows(Some(collect_window), LPARAM(std::ptr::from_mut(&mut handles) as isize)) }
        .context("enumerating top-level windows")?;
    Ok(handles)
}

unsafe extern "system" fn collect_window(hwnd: HWND, handles: LPARAM) -> BOOL {
    let handles = unsafe { &mut *(handles.0 as *mut Vec<HWND>) };
    handles.push(hwnd);
    BOOL::from(true)
}

fn app_window(hwnd: HWND, own_pid: u32) -> Option<Candidate> {
    if !is_visible(hwnd) {
        return None;
    }
    let class_name = class_name(hwnd);
    let popup_menu = class_name == POPUP_MENU_CLASS;
    if (!popup_menu && is_tool_window(hwnd)) || is_cloaked(hwnd) {
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
    Some(Candidate { hwnd, pid, title, class_name, rect })
}

fn may_be_untitled(class_name: &str) -> bool {
    matches!(class_name, DIALOG_CLASS | POPUP_MENU_CLASS)
}

fn describe(candidate: Candidate, foreground: Foreground, processes: &mut ProcessLookup) -> WindowInfo {
    let facts = processes.facts(candidate.pid);
    WindowInfo {
        hwnd: ref_of(candidate.hwnd),
        title: candidate.title,
        process: facts.name,
        pid: candidate.pid,
        class_name: candidate.class_name,
        rect: candidate.rect,
        state: show_state(candidate.hwnd),
        foreground: foreground.includes(candidate.hwnd),
        elevated: facts.elevated,
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn windows_have_visible_rects_and_exclude_this_process() {
        let list = windows().unwrap();
        assert!(!list.is_empty(), "no app windows found");
        assert!(list.iter().all(|info| !info.rect.is_empty()));
        assert!(list.iter().all(|info| info.pid != std::process::id()));
        assert!(list.iter().all(|info| !info.process.is_empty()));
    }

    #[test]
    fn foreground_window_exists() {
        assert!(foreground_window().is_some());
    }

    #[test]
    fn window_round_trips_the_first_window() {
        let list = windows().unwrap();
        let first = list.first().unwrap();
        let again = window(first.hwnd).unwrap();
        assert!(again.hwnd == first.hwnd, "hwnd differs");
        assert!(again.pid == first.pid, "pid differs");
        assert!(again.class_name == first.class_name, "class differs");
        assert!(again.process == first.process, "process differs");
        assert!(is_window(first.hwnd));
    }

    #[test]
    fn missing_window_is_not_found() {
        let error = window(WindowRef(0)).unwrap_err();
        assert_eq!(error.code, deckhand_core::ErrorCode::NotFound);
        assert!(!is_window(WindowRef(0)));
        assert!(owner_of(WindowRef(0)).is_none());
    }

    #[test]
    fn dialogs_and_popup_menus_may_be_untitled() {
        assert!(may_be_untitled("#32770"));
        assert!(may_be_untitled("#32768"));
        assert!(!may_be_untitled("tooltips_class32"));
        assert!(!may_be_untitled("Notepad"));
    }

    #[test]
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
    }
}
