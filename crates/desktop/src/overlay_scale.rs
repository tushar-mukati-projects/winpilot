const BASE_DPI: f32 = 96.0;
const MIN_SCALE: f32 = 1.0;
const MAX_SCALE: f32 = 4.0;

pub(crate) fn scale_from_dpi(dpi: u32) -> f32 {
    sane_scale(dpi as f32 / BASE_DPI)
}

pub(crate) fn sane_scale(scale: f32) -> f32 {
    if scale.is_finite() { scale.clamp(MIN_SCALE, MAX_SCALE) } else { MIN_SCALE }
}

pub(crate) fn scaled(length: f32, scale: f32) -> f32 {
    length * sane_scale(scale)
}

pub(crate) fn scaled_px(length: f32, scale: f32) -> i32 {
    scaled(length, scale).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dpi_maps_to_a_bounded_scale() {
        assert!(crate::easing::close(scale_from_dpi(96), 1.0));
        assert!(crate::easing::close(scale_from_dpi(144), 1.5));
        assert!(crate::easing::close(scale_from_dpi(0), 1.0));
        assert!(crate::easing::close(scale_from_dpi(9600), 4.0));
        assert!(crate::easing::close(sane_scale(f32::NAN), 1.0));
    }

    #[test]
    fn lengths_scale_and_round() {
        assert_eq!(scaled_px(10.0, 1.5), 15);
        assert_eq!(scaled_px(8.0, 1.25), 10);
    }
}
