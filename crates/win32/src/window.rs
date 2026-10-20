use std::ffi::c_void;

use windows::{
    Win32::{
        Foundation::{HWND, LPARAM, POINT, RECT, WPARAM},
        Graphics::Dwm::{DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute},
        UI::WindowsAndMessaging::{
            EnumWindows, GA_ROOT, GW_OWNER, GWL_EXSTYLE, GWL_STYLE, GetAncestor, GetClassNameW, GetForegroundWindow,
            GetSystemMetrics, GetWindow, GetWindowLongPtrW, GetWindowTextW, GetWindowThreadProcessId, IsIconic,
            IsWindow, IsWindowVisible, SEND_MESSAGE_TIMEOUT_FLAGS, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN,
            SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN, SYSTEM_METRICS_INDEX, SendMessageTimeoutW, WINDOW_EX_STYLE,
            WINDOW_STYLE, WindowFromPhysicalPoint,
        },
    },
    core::BOOL,
};
use winpilot_core::{Context, Error, Point, Rect, Result, WindowRef};

const TITLE_CAPACITY: usize = 1024;
const CLASS_CAPACITY: usize = 256;
const EXPECTED_TOP_LEVEL_WINDOWS: usize = 512;
const MAX_OWNERS: usize = 8;

pub fn hwnd_of(window: WindowRef) -> HWND {
    HWND(window.0 as isize as *mut c_void)
}

pub fn ref_of(hwnd: HWND) -> WindowRef {
    WindowRef(hwnd.0 as isize as i64)
}

pub fn hwnd_exists(hwnd: HWND) -> bool {
    !hwnd.is_invalid() && unsafe { IsWindow(Some(hwnd)) }.as_bool()
}

pub fn window_exists(window: WindowRef) -> bool {
    hwnd_exists(hwnd_of(window))
}

pub fn live_hwnd(window: WindowRef) -> Result<HWND> {
    let hwnd = hwnd_of(window);
    if hwnd_exists(hwnd) { Ok(hwnd) } else { Err(Error::not_found(format!("window {window} no longer exists"))) }
}

pub fn title(hwnd: HWND) -> String {
    let mut buffer = [0u16; TITLE_CAPACITY];
    let length = unsafe { GetWindowTextW(hwnd, &mut buffer) };
    utf16_prefix(&buffer, length)
}

pub fn class_name(hwnd: HWND) -> String {
    let mut buffer = [0u16; CLASS_CAPACITY];
    let length = unsafe { GetClassNameW(hwnd, &mut buffer) };
    utf16_prefix(&buffer, length)
}

pub fn owning_pid(hwnd: HWND) -> u32 {
    let mut pid = 0u32;
    let thread = unsafe { GetWindowThreadProcessId(hwnd, Some(&raw mut pid)) };
    if thread == 0 { 0 } else { pid }
}

pub fn owning_thread(hwnd: HWND) -> u32 {
    unsafe { GetWindowThreadProcessId(hwnd, None) }
}

pub fn root_of(window: WindowRef) -> Option<WindowRef> {
    root_window(hwnd_of(window))
}

pub fn owner_of(window: WindowRef) -> Option<WindowRef> {
    let owner = unsafe { GetWindow(hwnd_of(window), GW_OWNER) }.ok()?;
    (!owner.is_invalid()).then(|| ref_of(owner))
}

pub fn owner_chain(window: WindowRef) -> Vec<WindowRef> {
    let mut chain = Vec::new();
    let mut current = window;
    while let Some(owner) = owner_of(current) {
        if chain.contains(&owner) || chain.len() >= MAX_OWNERS {
            break;
        }
        chain.push(owner);
        current = owner;
    }
    chain
}

pub fn window_style(hwnd: HWND) -> WINDOW_STYLE {
    WINDOW_STYLE(unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } as u32)
}

pub fn window_ex_style(hwnd: HWND) -> WINDOW_EX_STYLE {
    WINDOW_EX_STYLE(unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) } as u32)
}

pub fn is_top_level(hwnd: HWND) -> bool {
    !hwnd.is_invalid() && unsafe { GetAncestor(hwnd, GA_ROOT) } == hwnd
}

pub fn window_at(point: Point) -> Option<WindowRef> {
    root_window(unsafe { WindowFromPhysicalPoint(POINT { x: point.x, y: point.y }) })
}

pub fn foreground_hwnd() -> Option<HWND> {
    let hwnd = unsafe { GetForegroundWindow() };
    (!hwnd.is_invalid()).then_some(hwnd)
}

pub fn foreground_root() -> Option<WindowRef> {
    foreground_hwnd().and_then(root_window)
}

pub fn is_foreground(window: WindowRef) -> bool {
    let Some(foreground) = foreground_hwnd() else { return false };
    window.0 != 0 && (ref_of(foreground) == window || root_window(foreground) == Some(window))
}

pub fn is_visible(hwnd: HWND) -> bool {
    unsafe { IsWindowVisible(hwnd) }.as_bool()
}

pub fn is_minimized(hwnd: HWND) -> bool {
    unsafe { IsIconic(hwnd) }.as_bool()
}

pub fn is_cloaked(hwnd: HWND) -> bool {
    cloak_flags(hwnd) != 0
}

pub fn cloak_flags(hwnd: HWND) -> u32 {
    let mut cloaked = 0u32;
    let size = size_of::<u32>() as u32;
    let read = unsafe { DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, (&raw mut cloaked).cast(), size) };
    if read.is_ok() { cloaked } else { 0 }
}

pub fn frame_bounds(hwnd: HWND) -> Option<Rect> {
    let mut rect = RECT::default();
    let size = size_of::<RECT>() as u32;
    unsafe { DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, (&raw mut rect).cast(), size) }.ok()?;
    Some(rect_from_win32(rect))
}

pub const fn rect_from_win32(rect: RECT) -> Rect {
    Rect::from_edges(rect.left, rect.top, rect.right, rect.bottom)
}

pub fn top_level_windows() -> Result<Vec<HWND>> {
    let mut handles: Vec<HWND> = Vec::with_capacity(EXPECTED_TOP_LEVEL_WINDOWS);
    unsafe { EnumWindows(Some(collect_window), LPARAM(std::ptr::from_mut(&mut handles) as isize)) }
        .context("enumerating top-level windows")?;
    Ok(handles)
}

pub fn virtual_screen() -> Rect {
    Rect::new(
        metric(SM_XVIRTUALSCREEN),
        metric(SM_YVIRTUALSCREEN),
        metric(SM_CXVIRTUALSCREEN),
        metric(SM_CYVIRTUALSCREEN),
    )
}

pub fn send_message_timeout(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    flags: SEND_MESSAGE_TIMEOUT_FLAGS,
    timeout_ms: u32,
) -> Option<usize> {
    let mut answer = 0usize;
    let sent = unsafe { SendMessageTimeoutW(hwnd, message, wparam, lparam, flags, timeout_ms, Some(&raw mut answer)) };
    (sent.0 != 0).then_some(answer)
}

fn utf16_prefix(buffer: &[u16], length: i32) -> String {
    let length = usize::try_from(length).unwrap_or(0).min(buffer.len());
    String::from_utf16_lossy(&buffer[..length])
}

fn root_window(handle: HWND) -> Option<WindowRef> {
    if handle.is_invalid() {
        return None;
    }
    let root = unsafe { GetAncestor(handle, GA_ROOT) };
    (!root.is_invalid()).then(|| ref_of(root))
}

unsafe extern "system" fn collect_window(hwnd: HWND, handles: LPARAM) -> BOOL {
    let handles = unsafe { &mut *(handles.0 as *mut Vec<HWND>) };
    handles.push(hwnd);
    BOOL::from(true)
}

fn metric(index: SYSTEM_METRICS_INDEX) -> i32 {
    unsafe { GetSystemMetrics(index) }
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, SMTO_ABORTIFHUNG, WM_NULL};
    use windows::core::w;
    use winpilot_core::ErrorCode;

    use super::*;

    #[test]
    fn window_refs_round_trip_through_hwnd() {
        let window = WindowRef(0x0012_3456);
        assert_eq!(ref_of(hwnd_of(window)), window);
        assert_eq!(ref_of(HWND(0x1234 as *mut c_void)), WindowRef(0x1234));
    }

    #[test]
    fn null_window_is_not_live() {
        assert!(!hwnd_exists(HWND::default()));
        assert!(!window_exists(WindowRef(0)));
        let error = live_hwnd(WindowRef(0)).unwrap_err();
        assert_eq!((error.code, error.message.as_str()), (ErrorCode::NotFound, "window w0 no longer exists"));
    }

    #[test]
    fn utf16_prefix_clamps_bad_lengths() {
        let buffer = [u16::from(b'a'), u16::from(b'b')];
        assert_eq!(utf16_prefix(&buffer, -1), "");
        assert_eq!(utf16_prefix(&buffer, 5), "ab");
    }

    #[test]
    fn null_window_has_no_owner_or_class() {
        assert_eq!(owning_pid(HWND::default()), 0);
        assert_eq!(owning_thread(HWND::default()), 0);
        assert_eq!(class_name(HWND::default()), "");
    }

    #[test]
    fn taskbar_has_a_process_and_class() {
        let Ok(taskbar) = (unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }) else { return };
        assert!(owning_pid(taskbar) > 0);
        assert_eq!(class_name(taskbar), "Shell_TrayWnd");
    }

    #[test]
    fn missing_windows_have_no_owners_or_style() {
        assert_eq!(owner_of(WindowRef(0)), None);
        assert!(owner_chain(WindowRef(0)).is_empty());
        assert_eq!((window_style(HWND::default()).0, window_ex_style(HWND::default()).0), (0, 0));
    }

    #[test]
    fn null_window_has_no_root_and_is_not_foreground() {
        assert_eq!(root_of(WindowRef(0)), None);
        assert!(!is_foreground(WindowRef(0)));
        assert!(!is_top_level(HWND::default()));
    }

    #[test]
    fn a_point_far_off_screen_has_no_window() {
        assert_eq!(window_at(Point::new(-90_000, -90_000)), None);
    }

    #[test]
    fn null_window_reads_as_hidden() {
        assert!(!is_visible(HWND::default()));
        assert!(!is_cloaked(HWND::default()));
        assert!(!is_minimized(HWND::default()));
    }

    #[test]
    fn null_window_has_no_frame() {
        assert!(frame_bounds(HWND::default()).is_none());
    }

    #[test]
    fn win32_rect_converts_to_edges() {
        let rect = RECT { left: -10, top: 5, right: 30, bottom: 25 };
        assert_eq!(rect_from_win32(rect), Rect::new(-10, 5, 40, 20));
        let rect = rect_from_win32(RECT { left: -1920, top: 10, right: 0, bottom: 1090 });
        assert_eq!(rect, Rect::new(-1920, 10, 1920, 1080));
    }

    #[test]
    fn the_desktop_has_top_level_windows() {
        assert!(!top_level_windows().unwrap().is_empty());
    }

    #[test]
    fn the_virtual_screen_has_an_area() {
        let screen = virtual_screen();
        assert!(screen.width > 0 && screen.height > 0);
    }

    #[test]
    fn a_null_window_never_answers() {
        let answer = send_message_timeout(HWND::default(), WM_NULL, WPARAM(0), LPARAM(0), SMTO_ABORTIFHUNG, 10);
        assert_eq!(answer, None);
    }
}
