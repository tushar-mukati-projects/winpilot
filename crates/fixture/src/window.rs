use std::ptr;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{COLOR_BTNFACE, HBRUSH};
use windows::Win32::UI::HiDpi::AdjustWindowRectExForDpi;
use windows::Win32::UI::WindowsAndMessaging::{
    CW_USEDEFAULT, CreateWindowExW, DestroyWindow, IDC_ARROW, IsWindow, LoadCursorW, RegisterClassExW, SW_SHOWDEFAULT,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SetWindowPos, ShowWindow, UnregisterClassW, WINDOW_EX_STYLE,
    WINDOW_STYLE, WNDCLASSEXW, WS_CAPTION, WS_CLIPCHILDREN, WS_EX_CONTROLPARENT, WS_MINIMIZEBOX, WS_OVERLAPPED,
    WS_SYSMENU,
};
use windows::core::{Error, HSTRING, PCWSTR, Result, w};

use crate::controls;
use crate::dpi::{BASE_DPI, scale, window_dpi};
use crate::layout::{CLIENT_HEIGHT, CLIENT_WIDTH};
use crate::options::Options;
use crate::params::high_word;
use crate::window_proc::window_proc;

const CLASS_NAME: PCWSTR = w!("WinPilotFixtureWindow");
const STYLE: WINDOW_STYLE =
    WINDOW_STYLE(WS_OVERLAPPED.0 | WS_CAPTION.0 | WS_SYSMENU.0 | WS_MINIMIZEBOX.0 | WS_CLIPCHILDREN.0);
const EXTENDED_STYLE: WINDOW_EX_STYLE = WS_EX_CONTROLPARENT;

pub struct WindowClass {
    instance: HINSTANCE,
}

impl WindowClass {
    pub fn register(instance: HINSTANCE) -> Result<Self> {
        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            hCursor: unsafe { LoadCursorW(None, IDC_ARROW)? },
            hbrBackground: HBRUSH(ptr::without_provenance_mut((COLOR_BTNFACE.0 + 1) as usize)),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        if unsafe { RegisterClassExW(&raw const class) } == 0 {
            return Err(Error::from_thread());
        }
        Ok(Self { instance })
    }
}

impl Drop for WindowClass {
    fn drop(&mut self) {
        let _unregistered = unsafe { UnregisterClassW(CLASS_NAME, Some(self.instance)) };
    }
}

pub struct MainWindow {
    handle: HWND,
}

impl MainWindow {
    pub fn create(class: &WindowClass, options: &Options) -> Result<Self> {
        let (x, y) = options.position.map_or((CW_USEDEFAULT, CW_USEDEFAULT), |position| (position.x, position.y));
        let size = outer_size(BASE_DPI)?;
        let title = HSTRING::from(&options.title);
        let handle = unsafe {
            CreateWindowExW(
                EXTENDED_STYLE,
                CLASS_NAME,
                &title,
                STYLE,
                x,
                y,
                size.cx,
                size.cy,
                None,
                None,
                Some(class.instance),
                None,
            )?
        };
        let window = Self { handle };
        controls::create_all(handle, class.instance)?;
        window.fit_to_current_dpi()?;
        Ok(window)
    }

    pub fn handle(&self) -> HWND {
        self.handle
    }

    pub fn show(&self) {
        let _was_visible = unsafe { ShowWindow(self.handle, SW_SHOWDEFAULT) };
    }

    fn fit_to_current_dpi(&self) -> Result<()> {
        let dpi = window_dpi(self.handle)?;
        controls::fit_to_dpi(self.handle, dpi)?;
        resize_for(self.handle, dpi)
    }
}

impl Drop for MainWindow {
    fn drop(&mut self) {
        if unsafe { IsWindow(Some(self.handle)) }.as_bool() {
            let _destroyed = unsafe { DestroyWindow(self.handle) };
        }
    }
}

pub fn on_dpi_changed(window: HWND, wparam: WPARAM, lparam: LPARAM) -> Result<()> {
    let dpi = u32::from(high_word(wparam.0));
    controls::fit_to_dpi(window, dpi)?;
    match suggested_frame(lparam) {
        Some(frame) => move_to(window, frame),
        None => resize_for(window, dpi),
    }
}

fn suggested_frame(lparam: LPARAM) -> Option<RECT> {
    let pointer = ptr::with_exposed_provenance::<RECT>(lparam.0 as usize);
    if pointer.is_null() {
        return None;
    }
    Some(unsafe { *pointer })
}

fn move_to(window: HWND, frame: RECT) -> Result<()> {
    let (width, height) = (frame.right - frame.left, frame.bottom - frame.top);
    unsafe { SetWindowPos(window, None, frame.left, frame.top, width, height, SWP_NOZORDER | SWP_NOACTIVATE) }
}

fn resize_for(window: HWND, dpi: u32) -> Result<()> {
    let size = outer_size(dpi)?;
    unsafe { SetWindowPos(window, None, 0, 0, size.cx, size.cy, SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE) }
}

fn outer_size(dpi: u32) -> Result<SIZE> {
    let mut frame = RECT { left: 0, top: 0, right: scale(CLIENT_WIDTH, dpi), bottom: scale(CLIENT_HEIGHT, dpi) };
    unsafe { AdjustWindowRectExForDpi(&raw mut frame, STYLE, false, EXTENDED_STYLE, dpi)? };
    Ok(SIZE { cx: frame.right - frame.left, cy: frame.bottom - frame.top })
}
