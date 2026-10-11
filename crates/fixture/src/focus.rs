use windows::Win32::Foundation::{HWND, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetFocus, IsWindowEnabled, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{IsChild, IsWindowVisible, WA_INACTIVE};

use crate::child::item;
use crate::ids::NAME_EDIT;
use crate::params::low_word;
use crate::state;

pub fn remember_on_deactivate(window: HWND, wparam: WPARAM) {
    if u32::from(low_word(wparam.0)) != WA_INACTIVE {
        return;
    }
    let focused = unsafe { GetFocus() };
    if !focused.is_invalid() && unsafe { IsChild(window, focused) }.as_bool() {
        state::remember_focus(focused);
    }
}

pub fn restore(window: HWND) {
    let candidates = [state::remembered_focus(), item(window, NAME_EDIT).ok()];
    for control in candidates.into_iter().flatten() {
        if is_focusable(window, control) && moved_focus_to(control) {
            return;
        }
    }
}

fn is_focusable(window: HWND, control: HWND) -> bool {
    unsafe {
        IsChild(window, control).as_bool() && IsWindowVisible(control).as_bool() && IsWindowEnabled(control).as_bool()
    }
}

fn moved_focus_to(control: HWND) -> bool {
    let _previous_focus = unsafe { SetFocus(Some(control)) };
    let focused = unsafe { GetFocus() };
    focused == control
}
