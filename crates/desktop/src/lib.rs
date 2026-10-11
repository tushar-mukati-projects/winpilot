pub mod apps;
pub mod changes;
pub mod clipboard;
pub mod overlay;
pub mod sync;

use std::time::Duration;

use deckhand_core::{Rect, Result, WindowInfo, WindowRef, WindowState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Monitor {
    pub index: usize,
    pub rect: Rect,
    pub work_area: Rect,
    pub scale_percent: u32,
    pub primary: bool,
}

pub fn enable_dpi_awareness() -> Result<()> {
    todo!()
}

pub fn windows() -> Result<Vec<WindowInfo>> {
    todo!()
}

pub fn window(hwnd: WindowRef) -> Result<WindowInfo> {
    todo!("{hwnd}")
}

pub fn is_window(hwnd: WindowRef) -> bool {
    todo!("{hwnd}")
}

pub fn foreground_window() -> Option<WindowRef> {
    todo!()
}

pub fn focus_window(hwnd: WindowRef) -> Result<()> {
    todo!("{hwnd}")
}

pub fn set_window_state(hwnd: WindowRef, state: WindowState) -> Result<()> {
    todo!("{hwnd} {state:?}")
}

pub fn close_window(hwnd: WindowRef) -> Result<()> {
    todo!("{hwnd}")
}

pub fn move_window(hwnd: WindowRef, rect: Rect) -> Result<()> {
    todo!("{hwnd} {rect:?}")
}

pub fn owner_of(hwnd: WindowRef) -> Option<WindowRef> {
    todo!("{hwnd}")
}

pub fn wait_until_responsive(hwnd: WindowRef, timeout: Duration) -> bool {
    todo!("{hwnd} {timeout:?}")
}

pub fn is_hung(hwnd: WindowRef) -> bool {
    todo!("{hwnd}")
}

pub fn own_process_elevated() -> bool {
    todo!()
}

pub fn secure_desktop_active() -> bool {
    todo!()
}

pub fn monitors() -> Result<Vec<Monitor>> {
    todo!()
}
