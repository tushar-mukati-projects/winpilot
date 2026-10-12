use deckhand_core::{Error, Point, Result};
use windows::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    SYSTEM_METRICS_INDEX,
};

const ABSOLUTE_RANGE: i64 = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VirtualDesktop {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl VirtualDesktop {
    pub(crate) fn current() -> Result<Self> {
        let desktop = Self {
            x: metric(SM_XVIRTUALSCREEN),
            y: metric(SM_YVIRTUALSCREEN),
            width: metric(SM_CXVIRTUALSCREEN),
            height: metric(SM_CYVIRTUALSCREEN),
        };
        if desktop.width <= 0 || desktop.height <= 0 {
            return Err(Error::internal("could not read the virtual screen size"));
        }
        Ok(desktop)
    }

    pub(crate) fn absolute(self, point: Point) -> (i32, i32) {
        (normalize_axis(point.x, self.x, self.width), normalize_axis(point.y, self.y, self.height))
    }
}

fn metric(index: SYSTEM_METRICS_INDEX) -> i32 {
    unsafe { GetSystemMetrics(index) }
}

fn normalize_axis(value: i32, origin: i32, extent: i32) -> i32 {
    let extent = i64::from(extent.max(1));
    let pixel = (i64::from(value) - i64::from(origin)).clamp(0, extent - 1);
    ((pixel * 2 + 1) * ABSOLUTE_RANGE / (extent * 2)) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    const ABSOLUTE_MAX: i64 = 65_535;

    const SINGLE: VirtualDesktop = VirtualDesktop { x: 0, y: 0, width: 2560, height: 1600 };
    const LEFT_MONITOR: VirtualDesktop = VirtualDesktop { x: -1920, y: 0, width: 4480, height: 1600 };
    const STACKED_ABOVE: VirtualDesktop = VirtualDesktop { x: 0, y: -1600, width: 2560, height: 3200 };

    fn pixel_by_truncation(absolute: i32, extent: i32) -> i64 {
        (i64::from(absolute) * i64::from(extent)) >> 16
    }

    fn pixel_by_rounding(absolute: i32, extent: i32) -> i64 {
        let scaled = i64::from(absolute) * i64::from(extent - 1);
        (scaled * 2 + ABSOLUTE_MAX) / (ABSOLUTE_MAX * 2)
    }

    #[test]
    fn single_screen_corners() {
        assert_eq!(SINGLE.absolute(Point::new(0, 0)), (12, 20));
        assert_eq!(SINGLE.absolute(Point::new(2559, 1599)), (65_523, 65_515));
        assert_eq!(SINGLE.absolute(Point::new(2559, 0)), (65_523, 20));
        assert_eq!(SINGLE.absolute(Point::new(0, 1599)), (12, 65_515));
    }

    #[test]
    fn single_screen_centre() {
        assert_eq!(SINGLE.absolute(Point::new(1280, 800)), (32_780, 32_788));
    }

    #[test]
    fn left_monitor_at_negative_origin() {
        assert_eq!(LEFT_MONITOR.absolute(Point::new(-1920, 0)), (7, 20));
        assert_eq!(LEFT_MONITOR.absolute(Point::new(0, 0)), (28_094, 20));
        assert_eq!(LEFT_MONITOR.absolute(Point::new(2559, 1599)), (65_528, 65_515));
    }

    #[test]
    fn vertically_stacked_monitors() {
        assert_eq!(STACKED_ABOVE.absolute(Point::new(0, -1600)), (12, 10));
        assert_eq!(STACKED_ABOVE.absolute(Point::new(0, 0)), (12, 32_778));
        assert_eq!(STACKED_ABOVE.absolute(Point::new(2559, 1599)), (65_523, 65_525));
    }

    #[test]
    fn every_pixel_maps_back_to_itself_under_truncation_and_rounding() {
        for extent in [1080, 1600, 1920, 2560, 3200, 3840, 4480] {
            for pixel in 0..extent {
                let absolute = normalize_axis(pixel, 0, extent);
                assert!((0..=65_535).contains(&absolute));
                assert_eq!(pixel_by_truncation(absolute, extent), i64::from(pixel), "extent {extent}");
                assert_eq!(pixel_by_rounding(absolute, extent), i64::from(pixel), "extent {extent}");
            }
        }
    }

    #[test]
    fn negative_origin_pixels_map_back_to_themselves() {
        for x in -1920..2560 {
            let (absolute, _) = LEFT_MONITOR.absolute(Point::new(x, 0));
            assert_eq!(pixel_by_truncation(absolute, LEFT_MONITOR.width) - 1920, i64::from(x));
        }
    }

    #[test]
    fn points_outside_the_desktop_are_clamped() {
        assert_eq!(SINGLE.absolute(Point::new(-5, 99_999)), (12, 65_515));
        assert_eq!(LEFT_MONITOR.absolute(Point::new(-4000, -10)), (7, 20));
        assert_eq!(SINGLE.absolute(Point::new(i32::MIN, i32::MAX)), (12, 65_515));
    }

    #[test]
    fn degenerate_desktop_does_not_divide_by_zero() {
        let tiny = VirtualDesktop { x: 0, y: 0, width: 1, height: 0 };
        assert_eq!(tiny.absolute(Point::new(0, 0)), (32_768, 32_768));
        assert_eq!(tiny.absolute(Point::new(5, 5)), (32_768, 32_768));
    }
}
