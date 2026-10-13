use windows::Win32::{
    Foundation::HWND,
    UI::{
        Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent},
        WindowsAndMessaging::{
            CHILDID_SELF, EVENT_OBJECT_CONTENTSCROLLED, EVENT_OBJECT_CREATE, EVENT_OBJECT_INVOKED,
            EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_NAMECHANGE, EVENT_OBJECT_REORDER, EVENT_OBJECT_SELECTION,
            EVENT_OBJECT_SHOW, EVENT_OBJECT_VALUECHANGE, EVENT_SYSTEM_FOREGROUND, MSG, OBJID_CARET, OBJID_CURSOR,
            OBJID_WINDOW, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
        },
    },
};
use winpilot_core::{Error, Result};

use crate::{
    activity::ChangeEvent,
    changes::board,
    event_time::win_event_instant,
    message_thread::ThreadMessages,
    window_identity::{is_top_level, owning_pid, ref_of},
};

const WATCHED_EVENT_RANGES: [(u32, u32); 5] = [
    (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
    (EVENT_OBJECT_CREATE, EVENT_OBJECT_REORDER),
    (EVENT_OBJECT_SELECTION, EVENT_OBJECT_NAMECHANGE),
    (EVENT_OBJECT_VALUECHANGE, EVENT_OBJECT_VALUECHANGE),
    (EVENT_OBJECT_INVOKED, EVENT_OBJECT_CONTENTSCROLLED),
];

struct WinEventHook(HWINEVENTHOOK);

impl WinEventHook {
    fn install(first_event: u32, last_event: u32) -> Result<Self> {
        let flags = WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS;
        let hook = unsafe { SetWinEventHook(first_event, last_event, None, Some(on_win_event), 0, 0, flags) };
        if hook.is_invalid() {
            return Err(Error::internal(format!("installing a WinEvent hook for {first_event:#x}..{last_event:#x}")));
        }
        Ok(Self(hook))
    }
}

impl Drop for WinEventHook {
    fn drop(&mut self) {
        let _unhooked = unsafe { UnhookWinEvent(self.0) };
    }
}

pub(crate) struct ChangeHooks {
    _hooks: Vec<WinEventHook>,
}

impl ThreadMessages for ChangeHooks {
    fn on_message(&mut self, _message: &MSG) -> bool {
        false
    }
}

pub(crate) fn install_change_hooks() -> Result<(ChangeHooks, ())> {
    let hooks = WATCHED_EVENT_RANGES
        .iter()
        .map(|&(first, last)| WinEventHook::install(first, last))
        .collect::<Result<Vec<_>>>()?;
    Ok((ChangeHooks { _hooks: hooks }, ()))
}

unsafe extern "system" fn on_win_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    object: i32,
    child: i32,
    _thread: u32,
    event_ms: u32,
) {
    if !is_meaningful(event, object) {
        return;
    }
    let pid = (!hwnd.is_invalid()).then(|| owning_pid(hwnd)).filter(|&pid| pid != 0);
    let opened = is_window_opening(event, hwnd, object, child).then(|| ref_of(hwnd));
    board().record(ChangeEvent { at: win_event_instant(event_ms), pid, opened });
}

fn is_meaningful(event: u32, object: i32) -> bool {
    if object == OBJID_CURSOR.0 || object == OBJID_CARET.0 {
        return false;
    }
    event != EVENT_OBJECT_LOCATIONCHANGE || object == OBJID_WINDOW.0
}

fn is_window_opening(event: u32, hwnd: HWND, object: i32, child: i32) -> bool {
    event == EVENT_OBJECT_SHOW && object == OBJID_WINDOW.0 && child == CHILDID_SELF as i32 && is_top_level(hwnd)
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::WindowsAndMessaging::{
        EVENT_OBJECT_DESTROY, EVENT_OBJECT_FOCUS, EVENT_OBJECT_HIDE, EVENT_OBJECT_SELECTIONADD,
        EVENT_OBJECT_SELECTIONREMOVE, EVENT_OBJECT_SELECTIONWITHIN, EVENT_OBJECT_STATECHANGE,
        EVENT_OBJECT_TEXTSELECTIONCHANGED, OBJID_CLIENT,
    };

    use super::*;

    fn watched(event: u32) -> bool {
        WATCHED_EVENT_RANGES.iter().any(|&(first, last)| (first..=last).contains(&event))
    }

    #[test]
    fn exactly_the_listed_events_are_hooked() {
        let wanted = [
            EVENT_SYSTEM_FOREGROUND,
            EVENT_OBJECT_CREATE,
            EVENT_OBJECT_DESTROY,
            EVENT_OBJECT_SHOW,
            EVENT_OBJECT_HIDE,
            EVENT_OBJECT_REORDER,
            EVENT_OBJECT_SELECTION,
            EVENT_OBJECT_SELECTIONADD,
            EVENT_OBJECT_SELECTIONREMOVE,
            EVENT_OBJECT_SELECTIONWITHIN,
            EVENT_OBJECT_STATECHANGE,
            EVENT_OBJECT_LOCATIONCHANGE,
            EVENT_OBJECT_NAMECHANGE,
            EVENT_OBJECT_VALUECHANGE,
            EVENT_OBJECT_INVOKED,
            EVENT_OBJECT_TEXTSELECTIONCHANGED,
            EVENT_OBJECT_CONTENTSCROLLED,
        ];
        assert!(wanted.iter().all(|&event| watched(event)));
        let hooked = (0..=0xFFFF).filter(|&event| watched(event)).count();
        assert_eq!(hooked, wanted.len());
        assert!(!watched(EVENT_OBJECT_FOCUS));
    }

    #[test]
    fn location_changes_count_only_for_windows() {
        assert!(is_meaningful(EVENT_OBJECT_LOCATIONCHANGE, OBJID_WINDOW.0));
        assert!(!is_meaningful(EVENT_OBJECT_LOCATIONCHANGE, OBJID_CLIENT.0));
        assert!(!is_meaningful(EVENT_OBJECT_LOCATIONCHANGE, 5));
    }

    #[test]
    fn cursor_and_caret_never_count() {
        assert!(!is_meaningful(EVENT_OBJECT_LOCATIONCHANGE, OBJID_CURSOR.0));
        assert!(!is_meaningful(EVENT_OBJECT_SHOW, OBJID_CURSOR.0));
        assert!(!is_meaningful(EVENT_OBJECT_HIDE, OBJID_CARET.0));
    }

    #[test]
    fn other_events_count_for_any_object() {
        assert!(is_meaningful(EVENT_OBJECT_NAMECHANGE, 12));
        assert!(is_meaningful(EVENT_OBJECT_VALUECHANGE, OBJID_CLIENT.0));
        assert!(is_meaningful(EVENT_SYSTEM_FOREGROUND, OBJID_WINDOW.0));
    }
}
