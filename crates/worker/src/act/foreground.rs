use std::{
    thread,
    time::{Duration, Instant},
};

use deckhand_core::{Error, ErrorCode, Result, WindowInfo, WindowRef, format::other_desktop};

use crate::window_classes::is_popup;

const FOREGROUND_SETTLE: Duration = Duration::from_millis(200);
const FOREGROUND_POLL: Duration = Duration::from_millis(10);
const MAX_OWNERS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Seen {
    foreground: Option<WindowRef>,
    owners_of_foreground: Vec<WindowRef>,
    target_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Front {
    Target,
    DialogOnTop,
    Elsewhere(Option<WindowRef>),
}

pub(crate) fn bring_forward(window: &WindowInfo) -> Result<WindowInfo> {
    let current = deckhand_desktop::window(window.hwnd)?;
    refuse_other_desktop(&current)?;
    if is_popup(&current) {
        require_popup_in_front(&current)?;
        return Ok(current);
    }
    let behind = deckhand_desktop::foreground_window() != Some(current.hwnd);
    if behind && deckhand_desktop::is_enabled(current.hwnd) {
        deckhand_desktop::focus_window(current.hwnd)?;
    }
    require_foreground(current.hwnd)?;
    deckhand_desktop::window(window.hwnd)
}

pub(crate) fn require_in_front(window: &WindowInfo) -> Result<()> {
    if is_popup(window) { require_popup_in_front(window) } else { require_foreground(window.hwnd) }
}

pub(crate) fn require_foreground(target: WindowRef) -> Result<()> {
    match front(target, &seen(target, settled_foreground(target))) {
        Front::Target => Ok(()),
        Front::DialogOnTop => Err(dialog_on_top(target)),
        Front::Elsewhere(other) => Err(Error::new(ErrorCode::NeedsUser, elsewhere_message(target, other))),
    }
}

pub(crate) fn require_no_dialog_on_top(target: WindowRef) -> Result<()> {
    if !deckhand_desktop::is_window(target) {
        return Ok(());
    }
    match front(target, &seen(target, deckhand_desktop::foreground_window())) {
        Front::DialogOnTop => Err(dialog_on_top(target)),
        Front::Target | Front::Elsewhere(_) => Ok(()),
    }
}

fn front(target: WindowRef, seen: &Seen) -> Front {
    if !seen.target_enabled {
        return Front::DialogOnTop;
    }
    match seen.foreground {
        Some(front) if front == target => Front::Target,
        Some(_) if seen.owners_of_foreground.contains(&target) => Front::DialogOnTop,
        other => Front::Elsewhere(other),
    }
}

fn popup_belongs_to(popup: &WindowInfo, owners: &[WindowRef], foreground: Option<&WindowInfo>) -> bool {
    foreground.is_some_and(|front| front.pid == popup.pid || owners.contains(&front.hwnd))
}

pub(crate) fn popup_showing(hwnd: WindowRef) -> bool {
    deckhand_desktop::all_visible_windows().is_ok_and(|windows| windows.iter().any(|window| window.hwnd == hwnd))
}

pub(crate) fn owner_chain(window: Option<WindowRef>) -> Vec<WindowRef> {
    let mut chain = Vec::new();
    let mut current = window;
    while let Some(owner) = current.and_then(deckhand_desktop::owner_of) {
        if chain.contains(&owner) || chain.len() >= MAX_OWNERS {
            break;
        }
        chain.push(owner);
        current = Some(owner);
    }
    chain
}

fn require_popup_in_front(popup: &WindowInfo) -> Result<()> {
    if !popup_showing(popup.hwnd) {
        return Err(Error::not_found(format!("{} is a menu or dropdown that is no longer showing", popup.hwnd)));
    }
    let foreground = deckhand_desktop::foreground_window().and_then(|hwnd| deckhand_desktop::window(hwnd).ok());
    if popup_belongs_to(popup, &owner_chain(Some(popup.hwnd)), foreground.as_ref()) {
        return Ok(());
    }
    let other = foreground.map(|window| window.hwnd);
    Err(Error::new(ErrorCode::NeedsUser, popup_elsewhere_message(popup.hwnd, other)))
}

fn refuse_other_desktop(window: &WindowInfo) -> Result<()> {
    let Some(desktop) = other_desktop(window) else { return Ok(()) };
    Err(Error::new(
        ErrorCode::NeedsUser,
        format!("{} is on {desktop}; Deckhand never switches desktops, so it won't bring it forward", window.hwnd),
    ))
}

fn seen(target: WindowRef, foreground: Option<WindowRef>) -> Seen {
    Seen {
        foreground,
        owners_of_foreground: owner_chain(foreground),
        target_enabled: deckhand_desktop::is_enabled(target),
    }
}

fn dialog_on_top(target: WindowRef) -> Error {
    Error::new(ErrorCode::NeedsUser, format!("a dialog is open on top of {target}; handle it first (observe shows it)"))
}

fn elsewhere_message(target: WindowRef, other: Option<WindowRef>) -> String {
    match other {
        Some(other) => format!("{target} is not in the foreground ({other} is); bring it forward or try again"),
        None => format!("{target} is not in the foreground; bring it forward or try again"),
    }
}

fn popup_elsewhere_message(popup: WindowRef, other: Option<WindowRef>) -> String {
    let front = other.map(|other| format!(" ({other} is)")).unwrap_or_default();
    format!("{popup} is a menu or dropdown of an app that is not in the foreground{front}; open it again first")
}

fn settled_foreground(target: WindowRef) -> Option<WindowRef> {
    let started = Instant::now();
    loop {
        let foreground = deckhand_desktop::foreground_window();
        if foreground == Some(target) || started.elapsed() >= FOREGROUND_SETTLE {
            return foreground;
        }
        thread::sleep(FOREGROUND_POLL);
    }
}

#[cfg(test)]
mod tests {
    use deckhand_core::Rect;

    use super::*;
    use crate::test_fixture::window;

    const TARGET: WindowRef = WindowRef(10);

    fn seen(foreground: Option<i64>, owners: &[i64], target_enabled: bool) -> Seen {
        Seen {
            foreground: foreground.map(WindowRef),
            owners_of_foreground: owners.iter().copied().map(WindowRef).collect(),
            target_enabled,
        }
    }

    fn app(hwnd: i64, pid: u32) -> WindowInfo {
        WindowInfo { pid, ..window(hwnd, "app.exe", "App", Rect::new(0, 0, 10, 10)) }
    }

    #[test]
    fn the_target_itself_in_front_is_fine() {
        assert_eq!(front(TARGET, &seen(Some(10), &[], true)), Front::Target);
    }

    #[test]
    fn an_owned_window_in_front_is_a_dialog_on_top() {
        assert_eq!(front(TARGET, &seen(Some(20), &[10], true)), Front::DialogOnTop);
        assert_eq!(front(TARGET, &seen(Some(30), &[20, 10], true)), Front::DialogOnTop);
    }

    #[test]
    fn a_disabled_target_has_a_modal_dialog_even_when_activated() {
        assert_eq!(front(TARGET, &seen(Some(10), &[], false)), Front::DialogOnTop);
    }

    #[test]
    fn an_unrelated_window_in_front_is_elsewhere() {
        assert_eq!(front(TARGET, &seen(Some(20), &[5], true)), Front::Elsewhere(Some(WindowRef(20))));
        assert_eq!(front(TARGET, &seen(None, &[], true)), Front::Elsewhere(None));
        assert!(elsewhere_message(TARGET, Some(WindowRef(20))).contains("(w20 is)"));
    }

    #[test]
    fn missing_windows_have_no_owners() {
        assert!(owner_chain(None).is_empty());
        assert!(owner_chain(Some(WindowRef(0))).is_empty());
    }

    #[test]
    fn a_popup_belongs_to_the_foreground_app_or_its_owner() {
        let menu = app(30, 7);
        assert!(popup_belongs_to(&menu, &[], Some(&app(10, 7))));
        assert!(popup_belongs_to(&menu, &[WindowRef(11)], Some(&app(11, 8))));
        assert!(!popup_belongs_to(&menu, &[WindowRef(11)], Some(&app(12, 8))));
        assert!(!popup_belongs_to(&menu, &[], None));
    }

    #[test]
    fn a_popup_of_a_background_app_says_who_is_in_front() {
        assert_eq!(
            popup_elsewhere_message(WindowRef(30), Some(WindowRef(12))),
            "w30 is a menu or dropdown of an app that is not in the foreground (w12 is); open it again first"
        );
        assert!(!popup_showing(WindowRef(0)));
    }

    #[test]
    fn windows_on_other_desktops_are_never_brought_forward() {
        let elsewhere = WindowInfo { desktop: Some(2), on_current_desktop: false, ..app(5, 1) };
        let error = refuse_other_desktop(&elsewhere).unwrap_err();
        assert_eq!(error.code, ErrorCode::NeedsUser);
        assert!(error.message.starts_with("w5 is on desktop 2; Deckhand never switches desktops"), "{}", error.message);
        let unknown = WindowInfo { desktop: None, ..elsewhere };
        assert!(refuse_other_desktop(&unknown).unwrap_err().message.starts_with("w5 is on another desktop"));
        assert!(refuse_other_desktop(&app(5, 1)).is_ok());
    }

    #[test]
    fn a_closed_window_has_no_dialog_on_top() {
        assert!(require_no_dialog_on_top(WindowRef(0)).is_ok());
    }
}
