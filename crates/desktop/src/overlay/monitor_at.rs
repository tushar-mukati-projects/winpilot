use windows::Win32::{
    Foundation::{POINT, RECT},
    Graphics::Gdi::{
        MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY, MonitorFromPoint, MonitorFromRect, MonitorFromWindow,
    },
};
use winpilot_core::{Point, Rect};
use winpilot_win32::foreground_hwnd;

use super::scale::scale_from_dpi;
use crate::monitors::{Monitor, describe_monitor, effective_dpi, monitors};

pub(crate) fn scale_at(point: Point) -> f32 {
    let monitor = unsafe { MonitorFromPoint(POINT { x: point.x, y: point.y }, MONITOR_DEFAULTTONEAREST) };
    scale_from_dpi(effective_dpi(monitor))
}

pub(crate) fn monitor_around(rect: Rect) -> Option<Monitor> {
    let area = RECT { left: rect.x, top: rect.y, right: rect.right(), bottom: rect.bottom() };
    describe_monitor(unsafe { MonitorFromRect(&raw const area, MONITOR_DEFAULTTONEAREST) })
}

pub(crate) fn work_area_around(rect: Rect) -> Option<Rect> {
    monitor_around(rect).map(|monitor| monitor.work_area)
}

pub(crate) fn foreground_work_area() -> Option<Rect> {
    let window = foreground_hwnd()?;
    describe_monitor(unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTOPRIMARY) }).map(|monitor| monitor.work_area)
}

pub(crate) fn primary_work_area() -> Option<Rect> {
    monitors().ok()?.into_iter().find(|monitor| monitor.primary).map(|monitor| monitor.work_area)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_primary_monitor_has_a_scale_and_a_work_area() {
        let area = primary_work_area().unwrap();
        assert!(!area.is_empty());
        assert!(scale_at(area.center()) >= 1.0);
        let primary = monitor_around(Rect::new(area.x + 10, area.y + 10, 200, 100)).unwrap();
        assert_eq!(primary.work_area, area);
        assert_eq!(work_area_around(Rect::new(area.x + 10, area.y + 10, 200, 100)), Some(area));
        assert!(monitor_around(Rect::new(-90_000, -90_000, 10, 10)).is_some());
        assert!(scale_at(Point::new(-90_000, -90_000)) >= 1.0);
        assert!(foreground_work_area().is_none_or(|work_area| !work_area.is_empty()));
    }
}
