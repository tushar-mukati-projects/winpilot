use std::ptr;

use windows::{
    Win32::{
        Foundation::{HWND, LPARAM},
        UI::WindowsAndMessaging::EnumChildWindows,
    },
    core::BOOL,
};
use winpilot_core::WindowRef;

use crate::window_identity::{class_name, live_hwnd, owning_pid};

const CORE_WINDOW_CLASS: &str = "Windows.UI.Core.CoreWindow";

struct ContentSearch {
    frame_pid: u32,
    content_pid: Option<u32>,
}

pub fn uwp_content_pid(frame: WindowRef) -> Option<u32> {
    let handle = live_hwnd(frame).ok()?;
    let mut search = ContentSearch { frame_pid: owning_pid(handle), content_pid: None };
    let _enumeration_result_is_meaningless =
        unsafe { EnumChildWindows(Some(handle), Some(visit_child), LPARAM(ptr::from_mut(&mut search) as isize)) };
    search.content_pid
}

unsafe extern "system" fn visit_child(hwnd: HWND, search: LPARAM) -> BOOL {
    let search = unsafe { &mut *(search.0 as *mut ContentSearch) };
    if class_name(hwnd) != CORE_WINDOW_CLASS {
        return BOOL::from(true);
    }
    let pid = owning_pid(hwnd);
    if pid == 0 || pid == search.frame_pid {
        return BOOL::from(true);
    }
    search.content_pid = Some(pid);
    BOOL::from(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_plain_windows_have_no_content_process() {
        assert_eq!(uwp_content_pid(WindowRef(0)), None);
        let plain = crate::window_list::windows()
            .unwrap()
            .into_iter()
            .find(|window| !window.process.eq_ignore_ascii_case("ApplicationFrameHost.exe"));
        if let Some(plain) = plain {
            assert_eq!(uwp_content_pid(plain.hwnd), None);
        }
    }
}
