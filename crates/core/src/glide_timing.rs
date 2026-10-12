use std::time::Duration;

use crate::geometry::Point;

const GLIDE_RATE_HZ: u32 = 120;
pub const MAX_GLIDE_MS: u64 = 3000;

fn ease_in_out_cubic(progress: f64) -> f64 {
    let t = if progress.is_finite() { progress.clamp(0.0, 1.0) } else { 1.0 };
    if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (2.0 - 2.0 * t).powi(3) / 2.0 }
}

pub fn glide_point(from: Point, to: Point, progress: f64) -> Point {
    let eased = ease_in_out_cubic(progress);
    let along = |start: i32, end: i32| (f64::from(start) + (f64::from(end) - f64::from(start)) * eased).round() as i32;
    Point::new(along(from.x, to.x), along(from.y, to.y))
}

pub fn capped_glide(duration: Duration) -> Duration {
    duration.min(Duration::from_millis(MAX_GLIDE_MS))
}

fn glide_steps(duration: Duration) -> u32 {
    ((capped_glide(duration).as_secs_f64() * f64::from(GLIDE_RATE_HZ)).ceil() as u32).max(1)
}

pub fn glide_path(from: Point, to: Point, duration: Duration) -> Vec<Point> {
    let steps = glide_steps(duration);
    (1..=steps).map(|step| glide_point(from, to, f64::from(step) / f64::from(steps))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn easing_starts_and_ends_slowly_and_is_symmetric() {
        let near = |value: f64, wanted: f64| (value - wanted).abs() < 1e-12;
        assert!(near(ease_in_out_cubic(0.0), 0.0));
        assert!(near(ease_in_out_cubic(0.5), 0.5));
        assert!(near(ease_in_out_cubic(1.0), 1.0));
        assert!(ease_in_out_cubic(0.1) < 0.01);
        assert!(near(ease_in_out_cubic(0.25) + ease_in_out_cubic(0.75), 1.0));
        assert!(near(ease_in_out_cubic(f64::NAN), 1.0));
        assert!(near(ease_in_out_cubic(-3.0), 0.0));
    }

    #[test]
    fn glides_are_capped_at_three_seconds() {
        assert_eq!(capped_glide(Duration::from_secs(60)), Duration::from_millis(MAX_GLIDE_MS));
        assert_eq!(capped_glide(Duration::from_millis(400)), Duration::from_millis(400));
    }

    #[test]
    fn glides_step_at_about_120_hz() {
        assert_eq!(glide_steps(Duration::ZERO), 1);
        assert_eq!(glide_steps(Duration::from_millis(120)), 15);
        assert_eq!(glide_steps(Duration::from_millis(1000)), 120);
        assert_eq!(glide_steps(Duration::from_secs(60)), 360);
    }

    #[test]
    fn the_path_ends_exactly_at_the_target_and_never_goes_back() {
        let (from, to) = (Point::new(100, 900), Point::new(1700, 200));
        let path = glide_path(from, to, Duration::from_millis(120));
        assert_eq!(path.len(), 15);
        assert_eq!(path.last(), Some(&to));
        assert!(path.windows(2).all(|pair| pair[1].x >= pair[0].x && pair[1].y <= pair[0].y));
        let first_step = path[0].x - from.x;
        let middle_step = path[8].x - path[7].x;
        assert!(first_step < middle_step);
    }

    #[test]
    fn an_instant_glide_is_one_jump() {
        assert_eq!(glide_path(Point::new(0, 0), Point::new(5, 5), Duration::ZERO), vec![Point::new(5, 5)]);
    }
}
