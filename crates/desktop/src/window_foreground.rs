use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{GA_ROOTOWNER, GetAncestor},
};
use winpilot_win32::foreground_hwnd;

fn root_owner(hwnd: HWND) -> HWND {
    unsafe { GetAncestor(hwnd, GA_ROOTOWNER) }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Foreground {
    window: Option<HWND>,
    root_owner: Option<HWND>,
}

impl Foreground {
    pub(crate) fn now() -> Self {
        let window = foreground_hwnd();
        let root_owner = window.map(root_owner).filter(|owner| !owner.is_invalid());
        Self { window, root_owner }
    }

    pub(crate) fn includes(self, hwnd: HWND) -> bool {
        self.window == Some(hwnd) || self.root_owner == Some(hwnd)
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::c_void;

    use super::*;

    #[test]
    fn foreground_includes_the_window_and_its_root_owner() {
        let window = HWND(0x10 as *mut c_void);
        let owner = HWND(0x20 as *mut c_void);
        let foreground = Foreground { window: Some(window), root_owner: Some(owner) };
        assert!(foreground.includes(window));
        assert!(foreground.includes(owner));
        assert!(!foreground.includes(HWND(0x30 as *mut c_void)));
    }
}
