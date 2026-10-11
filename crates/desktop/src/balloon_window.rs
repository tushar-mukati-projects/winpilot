use std::mem::size_of;

use deckhand_core::{Context, Result};
use windows::{
    Win32::{
        Foundation::{ERROR_CLASS_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Shell::{NIN_BALLOONHIDE, NIN_BALLOONTIMEOUT, NIN_BALLOONUSERCLICK},
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, HWND_MESSAGE, PostQuitMessage, RegisterClassExW,
                WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_TIMER, WNDCLASSEXW,
            },
        },
    },
    core::{PCWSTR, w},
};

const BALLOON_CLASS: PCWSTR = w!("DeckhandNotification");
const EVENT_MASK: isize = 0xFFFF;

pub(crate) const BALLOON_CALLBACK: u32 = WM_APP + 1;

#[derive(Debug)]
pub(crate) struct BalloonWindow(HWND);

impl BalloonWindow {
    pub(crate) fn create() -> Result<Self> {
        let instance = module_instance()?;
        register_balloon_class(instance)?;
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                BALLOON_CLASS,
                PCWSTR::null(),
                WINDOW_STYLE(0),
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                Some(instance),
                None,
            )
        }
        .context("creating the notification window")?;
        Ok(Self(hwnd))
    }

    pub(crate) const fn hwnd(&self) -> HWND {
        self.0
    }
}

impl Drop for BalloonWindow {
    fn drop(&mut self) {
        let _ = unsafe { DestroyWindow(self.0) };
    }
}

pub(crate) fn ends_balloon(message: u32, lparam: LPARAM) -> bool {
    match message {
        WM_TIMER => true,
        BALLOON_CALLBACK => {
            matches!(callback_event(lparam), NIN_BALLOONTIMEOUT | NIN_BALLOONUSERCLICK | NIN_BALLOONHIDE)
        }
        _ => false,
    }
}

fn callback_event(lparam: LPARAM) -> u32 {
    (lparam.0 & EVENT_MASK) as u32
}

fn module_instance() -> Result<HINSTANCE> {
    let module = unsafe { GetModuleHandleW(PCWSTR::null()) }.context("finding this module")?;
    Ok(module.into())
}

fn register_balloon_class(instance: HINSTANCE) -> Result<()> {
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(balloon_window_proc),
        hInstance: instance,
        lpszClassName: BALLOON_CLASS,
        ..WNDCLASSEXW::default()
    };
    if unsafe { RegisterClassExW(&raw const class) } != 0 {
        return Ok(());
    }
    let error = windows::core::Error::from_thread();
    if error.code() == ERROR_CLASS_ALREADY_EXISTS.to_hresult() {
        return Ok(());
    }
    Err(error).context("registering the notification window class")
}

unsafe extern "system" fn balloon_window_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if ends_balloon(message, lparam) {
        unsafe { PostQuitMessage(0) };
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::{
        Shell::{NIN_BALLOONSHOW, NIN_SELECT},
        WindowsAndMessaging::{WM_CONTEXTMENU, WM_MOUSEMOVE},
    };

    use super::*;

    fn callback(event: u32, icon_id: u32) -> LPARAM {
        LPARAM(((icon_id << 16) | event) as isize)
    }

    #[test]
    fn the_balloon_ends_on_timeout_click_hide_or_the_lifetime_timer() {
        for event in [NIN_BALLOONTIMEOUT, NIN_BALLOONUSERCLICK, NIN_BALLOONHIDE] {
            assert!(ends_balloon(BALLOON_CALLBACK, callback(event, 1)), "{event}");
            assert!(ends_balloon(BALLOON_CALLBACK, LPARAM(event as isize)), "version 0 form of {event}");
        }
        assert!(ends_balloon(WM_TIMER, LPARAM(0)));
    }

    #[test]
    fn other_callbacks_and_messages_keep_it() {
        for event in [NIN_BALLOONSHOW, WM_MOUSEMOVE, WM_CONTEXTMENU, NIN_SELECT] {
            assert!(!ends_balloon(BALLOON_CALLBACK, callback(event, 1)), "{event}");
        }
        assert!(!ends_balloon(WM_MOUSEMOVE, callback(NIN_BALLOONTIMEOUT, 1)));
        assert!(!ends_balloon(WM_APP, callback(NIN_BALLOONTIMEOUT, 1)));
    }

    #[test]
    fn a_message_only_window_is_created_and_destroyed() {
        let window = BalloonWindow::create().unwrap();
        assert!(!window.hwnd().is_invalid());
        let second = BalloonWindow::create().unwrap();
        assert_ne!(window.hwnd(), second.hwnd());
    }
}
