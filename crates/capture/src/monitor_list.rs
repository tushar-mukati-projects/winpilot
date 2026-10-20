use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{
    DEVMODEW, ENUM_CURRENT_SETTINGS, ENUM_DISPLAY_SETTINGS_FLAGS, EnumDisplayMonitors, EnumDisplaySettingsExW,
    GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
};
use windows::Win32::UI::WindowsAndMessaging::MONITORINFOF_PRIMARY;
use windows::core::{BOOL, PCWSTR};
use winpilot_core::{Context, Rect, Result};
use winpilot_win32::rect_from_win32;

const UNKNOWN_ORIENTATION: u32 = u32::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Monitor {
    pub id: isize,
    pub rect: Rect,
    pub primary: bool,
    pub orientation: u32,
}

pub(crate) fn order_key(primary: bool, rect: Rect) -> (bool, i32, i32) {
    (!primary, rect.x, rect.y)
}

pub(crate) fn enumerate_monitors() -> Result<Vec<Monitor>> {
    let mut handles: Vec<HMONITOR> = Vec::new();
    let collector = LPARAM(&raw mut handles as isize);
    unsafe { EnumDisplayMonitors(None, None, Some(collect_handle), collector) }.ok().context("EnumDisplayMonitors")?;
    Ok(handles.into_iter().filter_map(describe).collect())
}

unsafe extern "system" fn collect_handle(monitor: HMONITOR, _: HDC, _: *mut RECT, collector: LPARAM) -> BOOL {
    let handles = unsafe { &mut *(collector.0 as *mut Vec<HMONITOR>) };
    handles.push(monitor);
    BOOL::from(true)
}

pub(crate) fn monitor_info(monitor: HMONITOR) -> Option<MONITORINFOEXW> {
    let mut info = MONITORINFOEXW::default();
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    let described = unsafe { GetMonitorInfoW(monitor, (&raw mut info).cast::<MONITORINFO>()) };
    described.as_bool().then_some(info)
}

fn describe(monitor: HMONITOR) -> Option<Monitor> {
    let info = monitor_info(monitor)?;
    Some(Monitor {
        id: monitor.0 as isize,
        rect: rect_from_win32(info.monitorInfo.rcMonitor),
        primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
        orientation: orientation_of(&info.szDevice),
    })
}

fn orientation_of(device: &[u16; 32]) -> u32 {
    let mut mode = DEVMODEW { dmSize: size_of::<DEVMODEW>() as u16, ..DEVMODEW::default() };
    let name = PCWSTR(device.as_ptr());
    let read =
        unsafe { EnumDisplaySettingsExW(name, ENUM_CURRENT_SETTINGS, &raw mut mode, ENUM_DISPLAY_SETTINGS_FLAGS(0)) };
    if !read.as_bool() {
        return UNKNOWN_ORIENTATION;
    }
    unsafe { mode.Anonymous1.Anonymous2.dmDisplayOrientation }.0
}

#[cfg(test)]
mod tests {
    use winpilot_core::Point;

    use super::*;

    #[test]
    fn primary_comes_first_then_left_to_right_then_top_to_bottom() {
        let monitor = |x: i32, y: i32, primary: bool| (primary, Rect::new(x, y, 100, 100));
        let mut list = [monitor(100, 0, false), monitor(-100, 50, false), monitor(0, 0, true), monitor(-100, 0, false)];
        list.sort_by_key(|(primary, rect)| order_key(*primary, *rect));
        let order: Vec<(i32, i32)> = list.iter().map(|(_, rect)| (rect.x, rect.y)).collect();
        assert_eq!(order, [(0, 0), (-100, 0), (-100, 50), (100, 0)]);
    }

    #[test]
    fn live_monitors_have_one_primary_at_the_origin_and_a_known_orientation() {
        crate::dpi::make_thread_per_monitor_aware().unwrap();
        let monitors = enumerate_monitors().unwrap();
        let primaries: Vec<&Monitor> = monitors.iter().filter(|monitor| monitor.primary).collect();
        assert_eq!(primaries.len(), 1);
        assert_eq!(Point::new(primaries[0].rect.x, primaries[0].rect.y), Point::new(0, 0));
        assert!(monitors.iter().all(|monitor| monitor.orientation <= 3));
        assert_eq!(monitors, enumerate_monitors().unwrap());
    }
}
