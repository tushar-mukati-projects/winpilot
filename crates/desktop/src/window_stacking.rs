use windows::Win32::UI::WindowsAndMessaging::{
    SET_WINDOW_POS_FLAGS, SWP_ASYNCWINDOWPOS, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowPos,
};
use winpilot_core::{Context, Result, WindowRef};

use crate::{window_identity::live_hwnd, window_restore::ensure_responsive};

const BELOW_FLAGS: SET_WINDOW_POS_FLAGS =
    SET_WINDOW_POS_FLAGS(SWP_NOACTIVATE.0 | SWP_NOMOVE.0 | SWP_NOSIZE.0 | SWP_ASYNCWINDOWPOS.0);

pub fn place_below(hwnd: WindowRef, above: WindowRef) -> Result<()> {
    let handle = live_hwnd(hwnd)?;
    let anchor = live_hwnd(above)?;
    ensure_responsive(handle, hwnd)?;
    unsafe { SetWindowPos(handle, Some(anchor), 0, 0, 0, 0, BELOW_FLAGS) }
        .context(&format!("placing {hwnd} behind {above}"))
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::WindowsAndMessaging::SWP_NOOWNERZORDER;
    use winpilot_core::ErrorCode;

    use super::*;

    #[test]
    fn stacking_never_activates_moves_or_resizes() {
        for flag in [SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_ASYNCWINDOWPOS] {
            assert!(BELOW_FLAGS.contains(flag));
        }
        assert!(!BELOW_FLAGS.contains(SWP_NOOWNERZORDER));
    }

    #[test]
    fn missing_windows_are_rejected() {
        assert_eq!(place_below(WindowRef(0), WindowRef(0)).unwrap_err().code, ErrorCode::NotFound);
    }
}
