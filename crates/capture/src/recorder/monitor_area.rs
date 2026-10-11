use deckhand_core::{Error, Rect, Result};
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONULL, MONITORINFO, MonitorFromRect};

use crate::convert::rect_from_win32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MonitorArea {
    pub monitor: HMONITOR,
    pub monitor_rect: Rect,
    pub area: Rect,
}

impl MonitorArea {
    pub(crate) fn locate(wanted: Rect) -> Result<Self> {
        let edges = RECT { left: wanted.x, top: wanted.y, right: wanted.right(), bottom: wanted.bottom() };
        let monitor = unsafe { MonitorFromRect(&raw const edges, MONITOR_DEFAULTTONULL) };
        if monitor.is_invalid() {
            return Err(Error::invalid(format!(
                "region {},{} {}x{} is not on any monitor",
                wanted.x, wanted.y, wanted.width, wanted.height
            )));
        }
        let monitor_rect = monitor_rect(monitor)?;
        let area =
            wanted.intersection(monitor_rect).ok_or_else(|| Error::invalid("the region is not on any monitor"))?;
        Ok(Self { monitor, monitor_rect, area })
    }

    pub(crate) fn is_clipped(&self, wanted: Rect) -> bool {
        self.area != wanted
    }

    pub(crate) const fn area_in_monitor(&self) -> Rect {
        Rect::new(
            self.area.x - self.monitor_rect.x,
            self.area.y - self.monitor_rect.y,
            self.area.width,
            self.area.height,
        )
    }
}

fn monitor_rect(monitor: HMONITOR) -> Result<Rect> {
    let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..MONITORINFO::default() };
    if !unsafe { GetMonitorInfoW(monitor, &raw mut info) }.as_bool() {
        return Err(Error::internal("GetMonitorInfoW failed for the recorded monitor"));
    }
    Ok(rect_from_win32(info.rcMonitor))
}

#[cfg(test)]
mod tests {
    use deckhand_core::Point;

    use super::*;

    #[test]
    fn a_region_on_the_primary_monitor_keeps_its_rect() {
        crate::dpi::make_thread_per_monitor_aware().unwrap();
        let wanted = Rect::new(10, 20, 300, 200);
        let found = MonitorArea::locate(wanted).unwrap();
        assert!(found.monitor_rect.contains(Point::new(0, 0)));
        assert_eq!(found.area, wanted);
        assert!(!found.is_clipped(wanted));
        assert_eq!(found.area_in_monitor(), Rect::new(10 - found.monitor_rect.x, 20 - found.monitor_rect.y, 300, 200));
    }

    #[test]
    fn a_region_hanging_off_the_desktop_is_clipped_or_refused() {
        crate::dpi::make_thread_per_monitor_aware().unwrap();
        let primary = MonitorArea::locate(Rect::new(0, 0, 10, 10)).unwrap().monitor_rect;
        let hanging = Rect::new(primary.right() - 50, primary.y + 10, 100_000, 40);
        let found = MonitorArea::locate(hanging).unwrap();
        assert!(found.is_clipped(hanging));
        assert!(found.area.width <= found.monitor_rect.width);
        let nowhere = MonitorArea::locate(Rect::new(-90_000, -90_000, 10, 10)).unwrap_err();
        assert_eq!(nowhere.code, deckhand_core::ErrorCode::InvalidArgument);
    }
}
