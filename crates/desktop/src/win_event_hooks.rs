use std::time::Instant;

use deckhand_core::{Error, Result};
use windows::Win32::{
    Foundation::HWND,
    UI::{
        Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent},
        WindowsAndMessaging::{
            CHILDID_SELF, EVENT_OBJECT_CREATE, EVENT_OBJECT_END, EVENT_OBJECT_SHOW, EVENT_SYSTEM_FOREGROUND, MSG,
            OBJID_CURSOR, OBJID_WINDOW, WINEVENT_OUTOFCONTEXT, WINEVENT_SKIPOWNPROCESS,
        },
    },
};

use crate::{
    changes::board,
    message_thread::ThreadMessages,
    window_facts::{is_top_level, owning_pid, ref_of},
};

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
    _foreground: WinEventHook,
    _objects: WinEventHook,
}

impl ThreadMessages for ChangeHooks {
    fn on_message(&mut self, _message: &MSG) -> bool {
        false
    }
}

pub(crate) fn install_change_hooks() -> Result<(ChangeHooks, ())> {
    let foreground = WinEventHook::install(EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND)?;
    let objects = WinEventHook::install(EVENT_OBJECT_CREATE, EVENT_OBJECT_END)?;
    Ok((ChangeHooks { _foreground: foreground, _objects: objects }, ()))
}

unsafe extern "system" fn on_win_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    object: i32,
    child: i32,
    _thread: u32,
    _time: u32,
) {
    if object == OBJID_CURSOR.0 {
        return;
    }
    let pid = (!hwnd.is_invalid()).then(|| owning_pid(hwnd)).filter(|&pid| pid != 0);
    let opened = is_window_opening(event, hwnd, object, child).then(|| ref_of(hwnd));
    board().record(Instant::now(), pid, opened);
}

fn is_window_opening(event: u32, hwnd: HWND, object: i32, child: i32) -> bool {
    event == EVENT_OBJECT_SHOW && object == OBJID_WINDOW.0 && child == CHILDID_SELF as i32 && is_top_level(hwnd)
}
