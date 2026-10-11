use std::{
    thread,
    time::{Duration, Instant},
};

use deckhand_core::{Error, ErrorCode, Result, WindowInfo, WindowRef};

use crate::window_classes::is_popup;

const FOREGROUND_SETTLE: Duration = Duration::from_millis(200);
const FOREGROUND_POLL: Duration = Duration::from_millis(10);
const MAX_OWNERS: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub foreground: Option<WindowRef>,
    pub owners_of_foreground: Vec<WindowRef>,
    pub target_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Front {
    Target,
    DialogOnTop,
    Elsewhere(Option<WindowRef>),
}

pub fn bring_forward(window: &WindowInfo) -> Result<WindowInfo> {
    let current = deckhand_desktop::window(window.hwnd)?;
    if is_popup(&current) {
        return Ok(current);
    }
    let behind = deckhand_desktop::foreground_window() != Some(current.hwnd);
    if behind && deckhand_desktop::is_enabled(current.hwnd) {
        deckhand_desktop::focus_window(current.hwnd)?;
    }
    require_foreground(current.hwnd)?;
    deckhand_desktop::window(window.hwnd)
}

pub fn require_foreground(target: WindowRef) -> Result<()> {
    let foreground = settled_foreground(target);
    let seen = Seen {
        foreground,
        owners_of_foreground: owner_chain(foreground),
        target_enabled: deckhand_desktop::is_enabled(target),
    };
    match front(target, &seen) {
        Front::Target => Ok(()),
        Front::DialogOnTop => Err(Error::new(
            ErrorCode::NeedsUser,
            format!("a dialog is open on top of {target}; handle it first (observe shows it)"),
        )),
        Front::Elsewhere(other) => Err(Error::new(ErrorCode::NeedsUser, elsewhere_message(target, other))),
    }
}

pub fn front(target: WindowRef, seen: &Seen) -> Front {
    if !seen.target_enabled {
        return Front::DialogOnTop;
    }
    match seen.foreground {
        Some(front) if front == target => Front::Target,
        Some(_) if seen.owners_of_foreground.contains(&target) => Front::DialogOnTop,
        other => Front::Elsewhere(other),
    }
}

fn elsewhere_message(target: WindowRef, other: Option<WindowRef>) -> String {
    match other {
        Some(other) => format!("{target} is not in the foreground ({other} is); bring it forward or try again"),
        None => format!("{target} is not in the foreground; bring it forward or try again"),
    }
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

fn owner_chain(window: Option<WindowRef>) -> Vec<WindowRef> {
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

#[cfg(test)]
mod tests {
    use super::*;

    const TARGET: WindowRef = WindowRef(10);

    fn seen(foreground: Option<i64>, owners: &[i64], target_enabled: bool) -> Seen {
        Seen {
            foreground: foreground.map(WindowRef),
            owners_of_foreground: owners.iter().copied().map(WindowRef).collect(),
            target_enabled,
        }
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
}
