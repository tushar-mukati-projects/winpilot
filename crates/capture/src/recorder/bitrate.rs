use winpilot_core::protocol::Quality;

const MIN_BITRATE: f64 = 1_000_000.0;
const MAX_BITRATE: f64 = 60_000_000.0;

const fn bits_per_pixel(quality: Quality) -> f64 {
    match quality {
        Quality::Low => 0.05,
        Quality::Medium => 0.08,
        Quality::High => 0.12,
        Quality::Max => 0.2,
    }
}

pub(crate) fn average_bitrate(width: u32, height: u32, fps: u32, quality: Quality) -> u32 {
    let pixels_per_second = f64::from(width) * f64::from(height) * f64::from(fps);
    (pixels_per_second * bits_per_pixel(quality)).clamp(MIN_BITRATE, MAX_BITRATE).round() as u32
}

pub(crate) const fn gif_speed(quality: Quality) -> i32 {
    match quality {
        Quality::Low => 30,
        Quality::Medium => 20,
        Quality::High => 10,
        Quality::Max => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bitrate_scales_with_pixels_frames_and_quality() {
        assert_eq!(average_bitrate(1920, 1200, 30, Quality::Low), 3_456_000);
        assert_eq!(average_bitrate(1920, 1200, 30, Quality::Medium), 5_529_600);
        assert_eq!(average_bitrate(1920, 1200, 30, Quality::High), 8_294_400);
        assert_eq!(average_bitrate(1920, 1200, 30, Quality::Max), 13_824_000);
        assert_eq!(average_bitrate(1920, 1080, 60, Quality::High), 14_929_920);
    }

    #[test]
    fn bitrate_is_clamped_to_one_to_sixty_megabits() {
        assert_eq!(average_bitrate(320, 240, 5, Quality::Low), 1_000_000);
        assert_eq!(average_bitrate(2, 2, 1, Quality::Max), 1_000_000);
        assert_eq!(average_bitrate(3840, 2160, 60, Quality::Max), 60_000_000);
        assert_eq!(average_bitrate(u32::MAX, u32::MAX, u32::MAX, Quality::Max), 60_000_000);
    }

    #[test]
    fn gif_speed_trades_palette_effort_for_time_within_the_crate_limits() {
        let speeds = [Quality::Low, Quality::Medium, Quality::High, Quality::Max].map(gif_speed);
        assert_eq!(speeds, [30, 20, 10, 5]);
        assert!(speeds.iter().all(|speed| (1..=30).contains(speed)));
    }
}
