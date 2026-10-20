use std::ffi::c_void;

use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Gdi::HBRUSH,
        UI::{
            Input::HRAWINPUT,
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, HWND_MESSAGE, MSG, WINDOW_EX_STYLE, WINDOW_STYLE,
                WM_INPUT,
            },
        },
    },
    core::{PCWSTR, w},
};
use winpilot_core::{Context, Result};
use winpilot_win32::{ThreadMessages, module_instance, register_window_class};

use crate::{raw_input::handle_raw_input, raw_input_registration::RawInputRegistration};

const SINK_CLASS: PCWSTR = w!("WinPilotRawInputSink");

pub(crate) struct RawInputSink {
    _registration: RawInputRegistration,
    _window: MessageWindow,
}

impl RawInputSink {
    pub(crate) fn start() -> Result<Self> {
        let window = MessageWindow::create()?;
        let registration = RawInputRegistration::register(window.0)?;
        Ok(Self { _registration: registration, _window: window })
    }
}

impl ThreadMessages for RawInputSink {
    fn on_message(&mut self, _message: &MSG) -> bool {
        false
    }
}

struct MessageWindow(HWND);

impl MessageWindow {
    fn create() -> Result<Self> {
        let instance = module_instance()?;
        register_sink_class(instance)?;
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                SINK_CLASS,
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
        .context("creating the raw input window")?;
        Ok(Self(hwnd))
    }
}

impl Drop for MessageWindow {
    fn drop(&mut self) {
        let _ = unsafe { DestroyWindow(self.0) };
    }
}

fn register_sink_class(instance: HINSTANCE) -> Result<()> {
    register_window_class(instance, SINK_CLASS, Some(sink_window_proc), HBRUSH::default())
        .context("registering the raw input window class")
}

unsafe extern "system" fn sink_window_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if message == WM_INPUT {
        handle_raw_input(HRAWINPUT(lparam.0 as *mut c_void));
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}
