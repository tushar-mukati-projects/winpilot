use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, PostQuitMessage, WM_ACTIVATE, WM_COMMAND, WM_DESTROY, WM_DPICHANGED, WM_SETFOCUS, WM_TIMER,
};
use windows::core::Result;

use crate::appearance;
use crate::commands;
use crate::focus;
use crate::window;

const SUCCESS_EXIT_CODE: i32 = 0;
const FAILURE_EXIT_CODE: i32 = 1;

pub extern "system" fn window_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match handle(window, message, wparam, lparam) {
        Ok(true) => LRESULT(0),
        Ok(false) => unsafe { DefWindowProcW(window, message, wparam, lparam) },
        Err(_) => {
            unsafe { PostQuitMessage(FAILURE_EXIT_CODE) };
            LRESULT(0)
        }
    }
}

fn handle(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> Result<bool> {
    match message {
        WM_COMMAND => commands::on_command(window, wparam, lparam).map(|()| true),
        WM_TIMER => appearance::on_timer(window, wparam.0),
        WM_DPICHANGED => window::on_dpi_changed(window, wparam, lparam).map(|()| true),
        WM_ACTIVATE => {
            focus::remember_on_deactivate(window, wparam);
            Ok(false)
        }
        WM_SETFOCUS => {
            focus::restore(window);
            Ok(true)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(SUCCESS_EXIT_CODE) };
            Ok(true)
        }
        _ => Ok(false),
    }
}
