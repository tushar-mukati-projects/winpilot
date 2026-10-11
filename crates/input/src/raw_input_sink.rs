use std::{ffi::c_void, mem::size_of};

use deckhand_core::{Context, Result};
use windows::{
    Win32::{
        Foundation::{ERROR_CLASS_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            Input::HRAWINPUT,
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, HWND_MESSAGE, MSG, RegisterClassExW, WINDOW_EX_STYLE,
                WINDOW_STYLE, WM_INPUT, WNDCLASSEXW,
            },
        },
    },
    core::{PCWSTR, w},
};

use crate::{
    message_thread::ThreadMessages, raw_input::record_if_user_input, raw_input_registration::RawInputRegistration,
};

const SINK_CLASS: PCWSTR = w!("DeckhandRawInputSink");

pub struct RawInputSink {
    _registration: RawInputRegistration,
    _window: MessageWindow,
}

impl RawInputSink {
    pub fn start() -> Result<Self> {
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

fn module_instance() -> Result<HINSTANCE> {
    let module = unsafe { GetModuleHandleW(PCWSTR::null()) }.context("finding this module")?;
    Ok(module.into())
}

fn register_sink_class(instance: HINSTANCE) -> Result<()> {
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(sink_window_proc),
        hInstance: instance,
        lpszClassName: SINK_CLASS,
        ..WNDCLASSEXW::default()
    };
    if unsafe { RegisterClassExW(&raw const class) } != 0 {
        return Ok(());
    }
    let error = windows::core::Error::from_thread();
    if error.code() == ERROR_CLASS_ALREADY_EXISTS.to_hresult() {
        return Ok(());
    }
    Err(error).context("registering the raw input window class")
}

unsafe extern "system" fn sink_window_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if message == WM_INPUT {
        record_if_user_input(HRAWINPUT(lparam.0 as *mut c_void));
    }
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}
