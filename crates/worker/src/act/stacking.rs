use winpilot_core::{WindowInfo, WindowRef};

use super::foreground::owner_chain;
use crate::window_classes::is_shell;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Target {
    hwnd: WindowRef,
    topmost: bool,
    elsewhere: bool,
    owners: Vec<WindowRef>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Front {
    hwnd: WindowRef,
    topmost: bool,
    shell: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stacking {
    Leave,
    AlreadyBehind(WindowRef),
    PlaceBelow(WindowRef),
}

pub(crate) fn keep_behind_front(window: &WindowInfo, said_as: &str) -> Option<String> {
    let front = winpilot_desktop::foreground_window().and_then(|front| winpilot_desktop::window(front).ok());
    match stacking(&Target::of(window), front.as_ref().map(Front::of)) {
        Stacking::Leave => None,
        Stacking::AlreadyBehind(front) => Some(behind_note(said_as, front)),
        Stacking::PlaceBelow(front) => Some(placed_note(window.hwnd, front, said_as)),
    }
}

impl Target {
    fn of(window: &WindowInfo) -> Self {
        Self {
            hwnd: window.hwnd,
            topmost: winpilot_desktop::is_topmost(window.hwnd),
            elsewhere: !window.on_current_desktop,
            owners: owner_chain(Some(window.hwnd)),
        }
    }
}

impl Front {
    fn of(window: &WindowInfo) -> Self {
        Self { hwnd: window.hwnd, topmost: winpilot_desktop::is_topmost(window.hwnd), shell: is_shell(window) }
    }
}

fn stacking(target: &Target, front: Option<Front>) -> Stacking {
    let Some(front) = front.filter(|front| !stays_put(target, front)) else { return Stacking::Leave };
    if front.topmost && !target.topmost {
        return Stacking::AlreadyBehind(front.hwnd);
    }
    Stacking::PlaceBelow(front.hwnd)
}

fn stays_put(target: &Target, front: &Front) -> bool {
    target.elsewhere || front.shell || front.hwnd == target.hwnd || target.owners.contains(&front.hwnd)
}

fn placed_note(window: WindowRef, front: WindowRef, said_as: &str) -> String {
    match winpilot_desktop::place_below(window, front) {
        Ok(()) => behind_note(said_as, front),
        Err(error) => format!("WinPilot could not place {window} behind {front}: {}", error.message),
    }
}

fn behind_note(said_as: &str, front: WindowRef) -> String {
    format!("{said_as} behind {front}, the window the user is in")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(hwnd: i64) -> Target {
        Target { hwnd: WindowRef(hwnd), topmost: false, elsewhere: false, owners: Vec::new() }
    }

    fn front(hwnd: i64) -> Front {
        Front { hwnd: WindowRef(hwnd), topmost: false, shell: false }
    }

    #[test]
    fn a_window_goes_directly_below_the_users_window() {
        assert_eq!(stacking(&target(5), Some(front(9))), Stacking::PlaceBelow(WindowRef(9)));
        let topmost = Target { topmost: true, ..target(5) };
        assert_eq!(stacking(&topmost, Some(front(9))), Stacking::PlaceBelow(WindowRef(9)));
        let both = Front { topmost: true, ..front(9) };
        assert_eq!(stacking(&topmost, Some(both)), Stacking::PlaceBelow(WindowRef(9)));
    }

    #[test]
    fn an_always_on_top_users_window_already_covers_ordinary_windows() {
        let pinned = Front { topmost: true, ..front(9) };
        assert_eq!(stacking(&target(5), Some(pinned)), Stacking::AlreadyBehind(WindowRef(9)));
    }

    #[test]
    fn nothing_moves_without_a_users_window_to_stay_behind() {
        assert_eq!(stacking(&target(5), None), Stacking::Leave);
        assert_eq!(stacking(&target(5), Some(front(5))), Stacking::Leave);
        let desktop = Front { shell: true, ..front(9) };
        assert_eq!(stacking(&target(5), Some(desktop)), Stacking::Leave);
    }

    #[test]
    fn owned_windows_and_other_desktops_are_left_alone() {
        let dialog = Target { owners: vec![WindowRef(7), WindowRef(9)], ..target(5) };
        assert_eq!(stacking(&dialog, Some(front(9))), Stacking::Leave);
        let elsewhere = Target { elsewhere: true, ..target(5) };
        assert_eq!(stacking(&elsewhere, Some(front(9))), Stacking::Leave);
    }

    #[test]
    fn the_note_names_the_users_window() {
        assert_eq!(behind_note("it opened", WindowRef(9)), "it opened behind w9, the window the user is in");
    }
}
