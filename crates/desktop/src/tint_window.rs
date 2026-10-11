use deckhand_core::{Rect, Result};
use windows::Win32::{
    Foundation::{COLORREF, HINSTANCE},
    UI::WindowsAndMessaging::{
        HWND_TOPMOST, LWA_ALPHA, SWP_HIDEWINDOW, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_SHOWWINDOW,
        SetLayeredWindowAttributes, SetWindowPos,
    },
};

use crate::overlay_window::{Capture, OverlayWindow};

pub(crate) struct TintWindow {
    window: OverlayWindow,
    shown: Option<(Rect, u8)>,
}

impl TintWindow {
    pub(crate) fn create(instance: HINSTANCE) -> Result<(Self, bool)> {
        let window = OverlayWindow::create_tint(instance)?;
        let excluded = window.apply_capture(Capture::Excluded);
        Ok((Self { window, shown: None }, excluded))
    }

    pub(crate) fn show(&mut self, rect: Rect, alpha: u8) {
        if alpha == 0 || rect.is_empty() {
            self.hide();
            return;
        }
        if self.shown == Some((rect, alpha)) {
            return;
        }
        if self.present_then_show(rect, alpha) {
            self.shown = Some((rect, alpha));
        } else {
            self.shown = Some((rect, 0));
            self.hide();
        }
    }

    fn present_then_show(&self, rect: Rect, alpha: u8) -> bool {
        let hwnd = self.window.handle();
        if unsafe { SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA) }.is_err() {
            return false;
        }
        let flags = SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_SHOWWINDOW;
        unsafe { SetWindowPos(hwnd, Some(HWND_TOPMOST), rect.x, rect.y, rect.width, rect.height, flags) }.is_ok()
    }

    pub(crate) fn hide(&mut self) {
        if self.shown.take().is_none() {
            return;
        }
        let flags = SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_HIDEWINDOW;
        let _shrunk = unsafe { SetWindowPos(self.window.handle(), None, 0, 0, 1, 1, flags) };
    }
}
