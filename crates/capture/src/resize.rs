use std::borrow::Cow;

use deckhand_core::{Context, Result};
use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};

use crate::frame::Frame;

const EDGE_TOLERANCE_PX: f64 = 0.5;
const SEARCH_FRACTION: u32 = 10;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Fit {
    pub width: u32,
    pub height: u32,
    pub scale: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Candidate {
    limiting: u32,
    other: u32,
    scale: f64,
    edge_error: f64,
}

pub(crate) fn fit(width: u32, height: u32, max_width: u32, max_height: u32) -> Fit {
    let (max_width, max_height) = (max_width.max(1), max_height.max(1));
    let width_ratio = f64::from(width) / f64::from(max_width);
    let height_ratio = f64::from(height) / f64::from(max_height);
    if width_ratio <= 1.0 && height_ratio <= 1.0 {
        return Fit { width, height, scale: 1.0 };
    }
    if width_ratio >= height_ratio {
        let best = snapped(width, height, max_width, max_height);
        Fit { width: best.limiting, height: best.other, scale: best.scale }
    } else {
        let best = snapped(height, width, max_height, max_width);
        Fit { width: best.other, height: best.limiting, scale: best.scale }
    }
}

fn snapped(limiting_side: u32, other_side: u32, limiting_max: u32, other_max: u32) -> Candidate {
    let lowest = limiting_max.saturating_sub((limiting_max / SEARCH_FRACTION).max(1)).max(1);
    let [mut best, _] = candidates(limiting_side, other_side, limiting_max, other_max);
    for limiting in (lowest..=limiting_max).rev() {
        for candidate in candidates(limiting_side, other_side, limiting, other_max) {
            if candidate.edge_error <= EDGE_TOLERANCE_PX {
                return candidate;
            }
            if candidate.edge_error < best.edge_error {
                best = candidate;
            }
        }
    }
    best
}

fn candidates(limiting_side: u32, other_side: u32, limiting: u32, other_max: u32) -> [Candidate; 2] {
    let ideal = f64::from(other_side) * f64::from(limiting) / f64::from(limiting_side);
    let near = (ideal.round() as u32).clamp(1, other_max);
    let far =
        if f64::from(near) >= ideal { near.saturating_sub(1) } else { near.saturating_add(1) }.clamp(1, other_max);
    [balanced(limiting_side, other_side, limiting, near), balanced(limiting_side, other_side, limiting, far)]
}

fn balanced(limiting_side: u32, other_side: u32, limiting: u32, other: u32) -> Candidate {
    let (source_sum, target_sum) =
        (f64::from(limiting_side) + f64::from(other_side), f64::from(limiting) + f64::from(other));
    let scale = source_sum / target_sum;
    let edge_error = (f64::from(limiting) * scale - f64::from(limiting_side)).abs();
    Candidate { limiting, other, scale, edge_error }
}

pub(crate) fn fitted_pixels(frame: &Frame, fit: Fit) -> Result<Cow<'_, [u8]>> {
    if fit.width == frame.width && fit.height == frame.height {
        return Ok(Cow::Borrowed(&frame.bgra));
    }
    resize_bgra(&frame.bgra, (frame.width, frame.height), (fit.width, fit.height)).map(Cow::Owned)
}

pub(crate) fn resize_bgra(source: &[u8], from: (u32, u32), to: (u32, u32)) -> Result<Vec<u8>> {
    let source = ImageRef::new(from.0, from.1, source, PixelType::U8x4).context("preparing the resize source")?;
    let mut target = Image::new(to.0, to.1, PixelType::U8x4);
    let options = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3)).use_alpha(false);
    Resizer::new().resize(&source, &mut target, &options).context("resizing the image")?;
    Ok(target.into_vec())
}

#[cfg(test)]
mod tests {
    use deckhand_core::{ImageMapping, Point};

    use super::*;

    fn edge_errors(width: u32, height: u32, fit: Fit) -> (f64, f64) {
        let x = (f64::from(fit.width) * fit.scale - f64::from(width)).abs();
        let y = (f64::from(fit.height) * fit.scale - f64::from(height)).abs();
        (x, y)
    }

    fn worst_point_error_px(width: u32, height: u32, fit: Fit) -> f64 {
        let mapping = ImageMapping { origin: Point::new(0, 0), scale: fit.scale };
        let x_factor = f64::from(width) / f64::from(fit.width);
        let y_factor = f64::from(height) / f64::from(fit.height);
        let worst_x = (0..=fit.width as i32)
            .map(|x| (f64::from(mapping.to_screen(Point::new(x, 0)).x) - f64::from(x) * x_factor).abs())
            .fold(0.0, f64::max);
        let worst_y = (0..=fit.height as i32)
            .map(|y| (f64::from(mapping.to_screen(Point::new(0, y)).y) - f64::from(y) * y_factor).abs())
            .fold(0.0, f64::max);
        worst_x.max(worst_y)
    }

    #[test]
    fn fit_shrinks_to_the_tighter_limit() {
        assert_eq!(fit(2560, 1600, 1280, 1280), Fit { width: 1280, height: 800, scale: 2.0 });
        assert_eq!(fit(1000, 4000, 1280, 1280), Fit { width: 320, height: 1280, scale: 3.125 });
        let wide = fit(3840, 2160, 1568, 1568);
        assert_eq!((wide.width, wide.height), (1568, 882));
        assert!((wide.scale - 3840.0 / 1568.0).abs() < 1e-12);
    }

    #[test]
    fn fit_never_upscales() {
        assert_eq!(fit(800, 600, 1280, 1280), Fit { width: 800, height: 600, scale: 1.0 });
        assert_eq!(fit(1280, 800, 1280, 800), Fit { width: 1280, height: 800, scale: 1.0 });
    }

    #[test]
    fn fit_keeps_every_side_at_least_one_pixel_and_inside_the_limits() {
        let thin = fit(10_000, 1, 100, 100);
        assert_eq!((thin.width, thin.height), (100, 1));
        let odd = fit(1001, 3, 500, 500);
        assert!(odd.width <= 500 && odd.width >= 450 && odd.height == 1, "{odd:?}");
        let zero_limits = fit(10, 10, 0, 0);
        assert_eq!((zero_limits.width, zero_limits.height), (1, 1));
    }

    #[test]
    fn fit_edges_land_within_half_a_screen_pixel_on_awkward_sizes() {
        let awkward = [(1920, 1201), (2560, 1601), (3440, 1440), (1366, 768), (3000, 2001), (1646, 1648), (2561, 1599)];
        for (width, height) in awkward {
            let fitted = fit(width, height, 1280, 1280);
            let (x, y) = edge_errors(width, height, fitted);
            assert!(x <= 0.5 && y <= 0.5, "{width}x{height} -> {fitted:?}: edge errors {x} {y}");
            assert!(fitted.width <= 1280 && fitted.height <= 1280);
            assert!(fitted.width.max(fitted.height) >= 1152, "{fitted:?} shrank more than 10%");
        }
    }

    #[test]
    fn image_points_map_back_within_one_screen_pixel_on_awkward_sizes() {
        for (width, height) in [(1920, 1201), (2560, 1601), (3440, 1440), (1366, 768), (7680, 4321)] {
            let fitted = fit(width, height, 1280, 1280);
            let worst = worst_point_error_px(width, height, fitted);
            assert!(worst <= 1.0, "{width}x{height} -> {fitted:?}: worst point error {worst}");
        }
    }

    #[test]
    fn fit_edges_land_within_half_a_screen_pixel_for_screen_sizes_into_1280_and_1568() {
        for (max_width, max_height) in [(1280, 1280), (1568, 1568)] {
            for width in (640..=3840).step_by(23) {
                for height in (480..=2160).step_by(19) {
                    let fitted = fit(width, height, max_width, max_height);
                    let (x, y) = edge_errors(width, height, fitted);
                    assert!(x <= 0.5 && y <= 0.5, "{width}x{height} into {max_width}x{max_height}: {fitted:?}");
                }
            }
        }
    }

    #[test]
    fn fit_edges_are_never_off_by_more_than_half_an_image_pixel() {
        for (max_width, max_height) in [(1024, 768), (800, 600), (100, 100)] {
            for width in (1..=8000).step_by(97) {
                for height in (1..=5000).step_by(89) {
                    let fitted = fit(width, height, max_width, max_height);
                    let (x, y) = edge_errors(width, height, fitted);
                    let half_image_pixel = fitted.scale / 2.0 + 1e-9;
                    let other_side_is_real = f64::from(width.min(height)) >= fitted.scale;
                    assert!((1..=max_width).contains(&fitted.width), "{fitted:?}");
                    assert!((1..=max_height).contains(&fitted.height), "{fitted:?}");
                    assert!(!other_side_is_real || (x <= half_image_pixel && y <= half_image_pixel), "{fitted:?}");
                }
            }
        }
    }

    #[test]
    fn fitted_pixels_borrow_when_no_resize_is_needed() {
        let frame = Frame::black(Point::new(0, 0), 4, 2);
        let same = fitted_pixels(&frame, Fit { width: 4, height: 2, scale: 1.0 }).unwrap();
        assert!(matches!(same, Cow::Borrowed(_)));
        let half = fitted_pixels(&frame, Fit { width: 2, height: 1, scale: 2.0 }).unwrap();
        assert_eq!(half.len(), 2 * 4);
    }

    #[test]
    fn halves_a_flat_image() {
        let source = [10u8, 20, 30, 255].repeat(8 * 4);
        let resized = resize_bgra(&source, (8, 4), (4, 2)).unwrap();
        assert_eq!(resized.len(), 4 * 2 * 4);
        assert!(resized.as_chunks::<4>().0.iter().all(|pixel| *pixel == [10, 20, 30, 255]));
    }

    #[test]
    fn short_sources_are_rejected() {
        assert!(resize_bgra(&[0u8; 12], (2, 2), (1, 1)).is_err());
    }
}
