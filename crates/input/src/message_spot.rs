use std::{ffi::c_void, mem::size_of, ptr};

use windows::Win32::{
    Foundation::{HWND, POINT},
    Graphics::Gdi::ScreenToClient,
    UI::WindowsAndMessaging::{
        CWP_SKIPINVISIBLE, CWP_SKIPTRANSPARENT, ChildWindowFromPointEx, GUITHREADINFO, GetGUIThreadInfo,
        GetWindowThreadProcessId, IsChild, IsWindow,
    },
};
use winpilot_core::{Error, Point, Result, WindowRef};

const MAX_DEPTH: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MessageSpot {
    pub window: WindowRef,
    pub pid: u32,
    pub client: Point,
    pub screen: Point,
}

pub fn message_spot(top: WindowRef, screen: Point) -> Result<MessageSpot> {
    let deepest = deepest_child(live_hwnd(top)?, screen)?;
    Ok(MessageSpot {
        window: ref_of(deepest),
        pid: owning_pid(deepest),
        client: client_point(deepest, screen)?,
        screen,
    })
}

pub fn keyboard_window(top: WindowRef) -> Result<WindowRef> {
    let top = live_hwnd(top)?;
    let focus = thread_focus(top).filter(|focus| *focus == top || is_inside(top, *focus));
    Ok(ref_of(focus.unwrap_or(top)))
}

pub(crate) fn live_hwnd(window: WindowRef) -> Result<HWND> {
    let hwnd = HWND(window.0 as isize as *mut c_void);
    if !hwnd.is_invalid() && unsafe { IsWindow(Some(hwnd)) }.as_bool() {
        return Ok(hwnd);
    }
    Err(Error::not_found(format!("window {window} no longer exists")))
}

pub(crate) fn client_point(hwnd: HWND, screen: Point) -> Result<Point> {
    let mut point = POINT { x: screen.x, y: screen.y };
    if unsafe { ScreenToClient(hwnd, &raw mut point) }.as_bool() {
        return Ok(Point::new(point.x, point.y));
    }
    Err(Error::not_found(format!("window {} no longer exists", ref_of(hwnd))))
}

fn deepest_child(top: HWND, screen: Point) -> Result<HWND> {
    let mut current = top;
    for _ in 0..MAX_DEPTH {
        let Some(child) = child_at(current, client_point(current, screen)?) else { return Ok(current) };
        current = child;
    }
    Ok(current)
}

fn child_at(parent: HWND, client: Point) -> Option<HWND> {
    let point = POINT { x: client.x, y: client.y };
    let child = unsafe { ChildWindowFromPointEx(parent, point, CWP_SKIPINVISIBLE | CWP_SKIPTRANSPARENT) };
    (!child.is_invalid() && child != parent).then_some(child)
}

fn thread_focus(window: HWND) -> Option<HWND> {
    let thread = unsafe { GetWindowThreadProcessId(window, None) };
    if thread == 0 {
        return None;
    }
    let mut info = GUITHREADINFO { cbSize: size_of::<GUITHREADINFO>() as u32, ..GUITHREADINFO::default() };
    unsafe { GetGUIThreadInfo(thread, &raw mut info) }.ok()?;
    (!info.hwndFocus.is_invalid()).then_some(info.hwndFocus)
}

fn is_inside(parent: HWND, window: HWND) -> bool {
    unsafe { IsChild(parent, window) }.as_bool()
}

fn owning_pid(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    let thread = unsafe { GetWindowThreadProcessId(hwnd, Some(ptr::from_mut(&mut pid))) };
    if thread == 0 { 0 } else { pid }
}

fn ref_of(hwnd: HWND) -> WindowRef {
    WindowRef(hwnd.0 as isize as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_window_has_no_spot_and_no_keyboard_window() {
        let error = message_spot(WindowRef(0), Point::new(1, 1)).unwrap_err();
        assert_eq!(error.message, "window w0 no longer exists");
        assert!(keyboard_window(WindowRef(0)).is_err());
    }

    #[test]
    fn window_refs_round_trip_through_handles() {
        let hwnd = HWND(0x1234 as *mut c_void);
        assert_eq!(ref_of(hwnd), WindowRef(0x1234));
    }
}
