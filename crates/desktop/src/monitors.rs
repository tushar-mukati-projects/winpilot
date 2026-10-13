use std::mem::size_of;

use windows::{
    Win32::{
        Foundation::{LPARAM, RECT},
        Graphics::Gdi::{EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO},
        UI::{
            HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI},
            WindowsAndMessaging::{
                GetSystemMetrics, MONITORINFOF_PRIMARY, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
                SM_YVIRTUALSCREEN, SYSTEM_METRICS_INDEX,
            },
        },
    },
    core::BOOL,
};
use winpilot_core::{Error, Rect, Result};

use crate::frame::rect_from_win32;

const BASE_DPI: u32 = 96;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Monitor {
    pub index: usize,
    pub rect: Rect,
    pub work_area: Rect,
    pub scale_percent: u32,
    pub primary: bool,
}

pub fn monitors() -> Result<Vec<Monitor>> {
    let mut found: Vec<Monitor> = display_monitors()?.into_iter().filter_map(describe_monitor).collect();
    if found.is_empty() {
        return Err(Error::not_found("no display monitors are attached"));
    }
    sort_and_index(&mut found);
    Ok(found)
}

pub(crate) fn virtual_screen() -> Rect {
    Rect::new(
        metric(SM_XVIRTUALSCREEN),
        metric(SM_YVIRTUALSCREEN),
        metric(SM_CXVIRTUALSCREEN),
        metric(SM_CYVIRTUALSCREEN),
    )
}

fn metric(index: SYSTEM_METRICS_INDEX) -> i32 {
    unsafe { GetSystemMetrics(index) }
}

fn display_monitors() -> Result<Vec<HMONITOR>> {
    let mut handles: Vec<HMONITOR> = Vec::new();
    let listed = unsafe {
        EnumDisplayMonitors(None, None, Some(collect_monitor), LPARAM(std::ptr::from_mut(&mut handles) as isize))
    };
    if listed.as_bool() {
        Ok(handles)
    } else {
        Err(Error::internal(format!("enumerating monitors: {}", windows::core::Error::from_thread())))
    }
}

unsafe extern "system" fn collect_monitor(monitor: HMONITOR, _dc: HDC, _bounds: *mut RECT, handles: LPARAM) -> BOOL {
    let handles = unsafe { &mut *(handles.0 as *mut Vec<HMONITOR>) };
    handles.push(monitor);
    BOOL::from(true)
}

pub(crate) fn describe_monitor(monitor: HMONITOR) -> Option<Monitor> {
    let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..MONITORINFO::default() };
    if !unsafe { GetMonitorInfoW(monitor, &raw mut info) }.as_bool() {
        return None;
    }
    Some(Monitor {
        index: 0,
        rect: rect_from_win32(info.rcMonitor),
        work_area: rect_from_win32(info.rcWork),
        scale_percent: effective_dpi(monitor) * 100 / BASE_DPI,
        primary: info.dwFlags & MONITORINFOF_PRIMARY != 0,
    })
}

pub(crate) fn effective_dpi(monitor: HMONITOR) -> u32 {
    let (mut dpi_x, mut dpi_y) = (0u32, 0u32);
    let read = unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &raw mut dpi_x, &raw mut dpi_y) };
    if read.is_ok() && dpi_x > 0 { dpi_x } else { BASE_DPI }
}

fn sort_and_index(monitors: &mut [Monitor]) {
    monitors.sort_by_key(|monitor| (!monitor.primary, monitor.rect.x, monitor.rect.y));
    for (index, monitor) in monitors.iter_mut().enumerate() {
        monitor.index = index;
    }
}

#[cfg(test)]
mod tests {
    use winpilot_core::Point;

    use super::*;

    fn monitor(x: i32, y: i32, primary: bool) -> Monitor {
        let rect = Rect::new(x, y, 100, 100);
        Monitor { index: 99, rect, work_area: rect, scale_percent: 100, primary }
    }

    #[test]
    fn primary_comes_first_then_left_to_right_then_top_to_bottom() {
        let mut list =
            vec![monitor(100, 0, false), monitor(-100, 50, false), monitor(0, 0, true), monitor(-100, 0, false)];
        sort_and_index(&mut list);
        let order: Vec<(i32, i32)> = list.iter().map(|m| (m.rect.x, m.rect.y)).collect();
        assert_eq!(order, [(0, 0), (-100, 0), (-100, 50), (100, 0)]);
        assert!(list.iter().enumerate().all(|(index, m)| m.index == index));
    }

    #[test]
    fn real_monitors_put_primary_first_at_origin() {
        let list = monitors().unwrap();
        let primary = list.first().unwrap();
        assert!(primary.primary);
        assert_eq!(primary.index, 0);
        assert!(primary.rect.contains(Point::new(0, 0)));
        assert!(list.iter().all(|m| m.scale_percent >= 100));
        assert_eq!(list.iter().filter(|m| m.primary).count(), 1);
    }

    #[test]
    fn virtual_screen_covers_every_monitor() {
        let screen = virtual_screen();
        assert!(!screen.is_empty());
        for monitor in monitors().unwrap() {
            assert_eq!(monitor.rect.intersection(screen), Some(monitor.rect));
        }
    }
}
