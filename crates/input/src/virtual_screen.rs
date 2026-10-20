use winpilot_core::{Error, Point, Rect, Result};
use winpilot_win32::virtual_screen;

const ABSOLUTE_RANGE: i64 = 65_536;

pub(crate) fn current_screen() -> Result<Rect> {
    let screen = virtual_screen();
    if screen.is_empty() {
        return Err(Error::internal("could not read the virtual screen size"));
    }
    Ok(screen)
}

pub(crate) fn absolute(screen: Rect, point: Point) -> (i32, i32) {
    (normalize_axis(point.x, screen.x, screen.width), normalize_axis(point.y, screen.y, screen.height))
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

    const SINGLE: Rect = Rect::new(0, 0, 2560, 1600);
    const LEFT_MONITOR: Rect = Rect::new(-1920, 0, 4480, 1600);
    const STACKED_ABOVE: Rect = Rect::new(0, -1600, 2560, 3200);

    fn pixel_by_truncation(absolute: i32, extent: i32) -> i64 {
        (i64::from(absolute) * i64::from(extent)) >> 16
    }

    fn pixel_by_rounding(absolute: i32, extent: i32) -> i64 {
        let scaled = i64::from(absolute) * i64::from(extent - 1);
        (scaled * 2 + ABSOLUTE_MAX) / (ABSOLUTE_MAX * 2)
    }

    #[test]
    fn single_screen_corners() {
        assert_eq!(absolute(SINGLE, Point::new(0, 0)), (12, 20));
        assert_eq!(absolute(SINGLE, Point::new(2559, 1599)), (65_523, 65_515));
        assert_eq!(absolute(SINGLE, Point::new(2559, 0)), (65_523, 20));
        assert_eq!(absolute(SINGLE, Point::new(0, 1599)), (12, 65_515));
    }

    #[test]
    fn single_screen_centre() {
        assert_eq!(absolute(SINGLE, Point::new(1280, 800)), (32_780, 32_788));
    }

    #[test]
    fn left_monitor_at_negative_origin() {
        assert_eq!(absolute(LEFT_MONITOR, Point::new(-1920, 0)), (7, 20));
        assert_eq!(absolute(LEFT_MONITOR, Point::new(0, 0)), (28_094, 20));
        assert_eq!(absolute(LEFT_MONITOR, Point::new(2559, 1599)), (65_528, 65_515));
    }

    #[test]
    fn vertically_stacked_monitors() {
        assert_eq!(absolute(STACKED_ABOVE, Point::new(0, -1600)), (12, 10));
        assert_eq!(absolute(STACKED_ABOVE, Point::new(0, 0)), (12, 32_778));
        assert_eq!(absolute(STACKED_ABOVE, Point::new(2559, 1599)), (65_523, 65_525));
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
            let (mapped, _) = absolute(LEFT_MONITOR, Point::new(x, 0));
            assert_eq!(pixel_by_truncation(mapped, LEFT_MONITOR.width) - 1920, i64::from(x));
        }
    }

    #[test]
    fn points_outside_the_screen_are_clamped() {
        assert_eq!(absolute(SINGLE, Point::new(-5, 99_999)), (12, 65_515));
        assert_eq!(absolute(LEFT_MONITOR, Point::new(-4000, -10)), (7, 20));
        assert_eq!(absolute(SINGLE, Point::new(i32::MIN, i32::MAX)), (12, 65_515));
    }

    #[test]
    fn degenerate_screen_does_not_divide_by_zero() {
        let tiny = Rect::new(0, 0, 1, 0);
        assert_eq!(absolute(tiny, Point::new(0, 0)), (32_768, 32_768));
        assert_eq!(absolute(tiny, Point::new(5, 5)), (32_768, 32_768));
    }
}
