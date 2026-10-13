use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, HMONITOR, MONITOR_DEFAULTTONULL, MONITORINFO, MonitorFromRect};
use winpilot_core::{Error, Rect, Result};

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

    pub(crate) fn follow_monitor(&mut self) -> Result<()> {
        let current = monitor_rect(self.monitor)?;
        if current != self.monitor_rect {
            *self = self.moved_to(current);
        }
        Ok(())
    }

    fn moved_to(&self, monitor_rect: Rect) -> Self {
        let area = if self.area == self.monitor_rect {
            monitor_rect
        } else {
            let relative = self.area_in_monitor();
            Rect::new(monitor_rect.x + relative.x, monitor_rect.y + relative.y, relative.width, relative.height)
        };
        Self { monitor: self.monitor, monitor_rect, area }
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
    use winpilot_core::Point;

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
    fn a_whole_monitor_follows_a_new_resolution_and_a_region_keeps_its_place_on_the_monitor() {
        let monitor = HMONITOR(std::ptr::dangling_mut());
        let whole =
            MonitorArea { monitor, monitor_rect: Rect::new(0, 0, 2560, 1600), area: Rect::new(0, 0, 2560, 1600) };
        let resized = whole.moved_to(Rect::new(0, 0, 1920, 1080));
        assert_eq!(resized.area, Rect::new(0, 0, 1920, 1080));
        let region = MonitorArea {
            monitor,
            monitor_rect: Rect::new(-1920, 0, 1920, 1080),
            area: Rect::new(-1820, 50, 400, 300),
        };
        let rearranged = region.moved_to(Rect::new(2560, 200, 1920, 1080));
        assert_eq!(rearranged.monitor_rect, Rect::new(2560, 200, 1920, 1080));
        assert_eq!(rearranged.area, Rect::new(2660, 250, 400, 300));
        assert_eq!(rearranged.area_in_monitor(), region.area_in_monitor());
    }

    #[test]
    fn following_an_unchanged_monitor_keeps_the_area() {
        crate::dpi::make_thread_per_monitor_aware().unwrap();
        let mut found = MonitorArea::locate(Rect::new(10, 20, 300, 200)).unwrap();
        let before = found;
        found.follow_monitor().unwrap();
        assert_eq!(found, before);
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
        assert_eq!(nowhere.code, winpilot_core::ErrorCode::InvalidArgument);
    }
}
