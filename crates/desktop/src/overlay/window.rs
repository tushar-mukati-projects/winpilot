use std::sync::atomic::{AtomicBool, Ordering};

use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Gdi::{GetStockObject, HBRUSH, WHITE_BRUSH},
        UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, HTTRANSPARENT, HWND_TOPMOST, MA_NOACTIVATE, SW_HIDE,
            SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SetWindowDisplayAffinity,
            SetWindowPos, ShowWindow, WDA_EXCLUDEFROMCAPTURE, WM_MOUSEACTIVATE, WM_NCHITTEST, WS_EX_LAYERED,
            WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
        },
    },
    core::{PCWSTR, w},
};
use winpilot_core::{Context, Result};

use crate::window_class::register_window_class;

const OVERLAY_CLASS: PCWSTR = w!("WinPilotOverlay");
const TINT_CLASS: PCWSTR = w!("WinPilotOverlayTint");
const OVERLAY_TITLE: PCWSTR = w!("WinPilot overlay");

static CLASS_REGISTERED: AtomicBool = AtomicBool::new(false);
static TINT_CLASS_REGISTERED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Capture {
    Excluded,
    Allowed,
}

pub(crate) struct OverlayWindow(HWND);

impl OverlayWindow {
    pub(crate) fn create(instance: HINSTANCE) -> Result<Self> {
        Self::of_class(instance, OVERLAY_CLASS)
    }

    pub(crate) fn create_tint(instance: HINSTANCE) -> Result<Self> {
        Self::of_class(instance, TINT_CLASS)
    }

    fn of_class(instance: HINSTANCE, class: PCWSTR) -> Result<Self> {
        let style = WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
        let hwnd = unsafe {
            CreateWindowExW(style, class, OVERLAY_TITLE, WS_POPUP, 0, 0, 0, 0, None, None, Some(instance), None)
        }
        .context("creating an overlay window")?;
        Ok(Self(hwnd))
    }

    pub(crate) fn handle(&self) -> HWND {
        self.0
    }

    pub(crate) fn apply_capture(&self, wanted: Capture) -> Capture {
        let applied = match wanted {
            Capture::Excluded => unsafe { SetWindowDisplayAffinity(self.0, WDA_EXCLUDEFROMCAPTURE) }.is_ok(),
            Capture::Allowed => true,
        };
        capture_in_effect(wanted, applied)
    }

    pub(crate) fn show_on_top(&self) {
        let _was_visible = unsafe { ShowWindow(self.0, SW_SHOWNOACTIVATE) };
        self.raise();
    }

    pub(crate) fn raise(&self) {
        let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER;
        let _raised = unsafe { SetWindowPos(self.0, Some(HWND_TOPMOST), 0, 0, 0, 0, flags) };
    }

    pub(crate) fn hide(&self) {
        let _was_visible = unsafe { ShowWindow(self.0, SW_HIDE) };
    }
}

impl Drop for OverlayWindow {
    fn drop(&mut self) {
        let _ = unsafe { DestroyWindow(self.0) };
    }
}

fn capture_in_effect(wanted: Capture, applied: bool) -> Capture {
    if applied { wanted } else { Capture::Allowed }
}

pub(crate) fn ensure_classes_registered(instance: HINSTANCE) -> Result<()> {
    register_once(&CLASS_REGISTERED, instance, OVERLAY_CLASS, HBRUSH::default())?;
    let white = HBRUSH(unsafe { GetStockObject(WHITE_BRUSH) }.0);
    register_once(&TINT_CLASS_REGISTERED, instance, TINT_CLASS, white)
}

fn register_once(registered: &AtomicBool, instance: HINSTANCE, name: PCWSTR, background: HBRUSH) -> Result<()> {
    if registered.load(Ordering::Acquire) {
        return Ok(());
    }
    register_window_class(instance, name, Some(overlay_window_proc), background)
        .context("registering an overlay window class")?;
    registered.store(true, Ordering::Release);
    Ok(())
}

unsafe extern "system" fn overlay_window_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_exclusion_leaves_the_window_capturable() {
        assert_eq!(capture_in_effect(Capture::Excluded, true), Capture::Excluded);
        assert_eq!(capture_in_effect(Capture::Excluded, false), Capture::Allowed);
        assert_eq!(capture_in_effect(Capture::Allowed, true), Capture::Allowed);
    }
}
