use deckhand_core::{Point, Rect};
use windows::Win32::{
    Foundation::POINT,
    Graphics::Gdi::{MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTONULL, MonitorFromPoint},
};

use crate::{
    monitors::{effective_dpi, monitors},
    overlay_scale::scale_from_dpi,
};

pub(crate) fn scale_at(point: Point) -> f32 {
    let monitor = unsafe { MonitorFromPoint(POINT { x: point.x, y: point.y }, MONITOR_DEFAULTTONEAREST) };
    scale_from_dpi(effective_dpi(monitor))
}

pub(crate) fn on_a_monitor(point: Point) -> bool {
    !unsafe { MonitorFromPoint(POINT { x: point.x, y: point.y }, MONITOR_DEFAULTTONULL) }.is_invalid()
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
        assert!(on_a_monitor(area.center()));
        assert!(scale_at(area.center()) >= 1.0);
        assert!(!on_a_monitor(Point::new(-90_000, -90_000)));
        assert!(scale_at(Point::new(-90_000, -90_000)) >= 1.0);
    }
}
