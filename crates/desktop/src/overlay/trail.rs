use std::time::Duration;

use deckhand_core::{Point, glide_timing::glide_point};

use super::{
    canvas::Canvas,
    color::{Premul, Rgb, WHITE},
    easing::{lerp, progress},
    rounded_box::coverage,
    scale::scaled,
};

pub(crate) const TRAIL_LIFE: Duration = Duration::from_millis(260);
pub(crate) const MAX_TRAIL_DOTS: usize = 10;
const MIN_TRAIL_DOTS: usize = 6;
const MIN_DISTANCE: f64 = 24.0;
const SPACING: f64 = 48.0;
const TAIL_RADIUS: f32 = 2.0;
const HEAD_RADIUS: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TrailDot {
    pub(crate) center: Point,
    pub(crate) radius: f32,
    pub(crate) peak: f32,
    pub(crate) delay: Duration,
    pub(crate) life: Duration,
}

impl TrailDot {
    pub(crate) fn alpha(&self, elapsed: Duration) -> f32 {
        let Some(age) = elapsed.checked_sub(self.delay) else { return 0.0 };
        self.peak * (1.0 - progress(age, self.life))
    }

    pub(crate) fn ends(&self) -> Duration {
        self.delay + self.life
    }

    pub(crate) fn reach(&self) -> i32 {
        (self.radius * 2.6 + 1.5).ceil() as i32
    }
}

pub(crate) fn trail_dots(from: Point, to: Point, scale: f32) -> Vec<TrailDot> {
    let Some(count) = dot_count(from, to) else { return Vec::new() };
    (0..count)
        .map(|index| {
            let along = f64::from(fraction(index, count));
            let center = Point::new(along_axis(from.x, to.x, along), along_axis(from.y, to.y, along));
            let headward = headward(index, count);
            TrailDot { center, life: TRAIL_LIFE.mul_f32(lerp(0.55, 1.0, headward)), ..sized(headward, scale) }
        })
        .collect()
}

pub(crate) fn glide_dots(from: Point, to: Point, duration: Duration, scale: f32) -> Vec<TrailDot> {
    let Some(count) = dot_count(from, to) else { return Vec::new() };
    (0..count)
        .map(|index| {
            let along = fraction(index, count);
            let center = glide_point(from, to, f64::from(along));
            TrailDot { center, delay: duration.mul_f32(along), ..sized(headward(index, count), scale) }
        })
        .collect()
}

pub(crate) fn trail_ends(dots: &[TrailDot]) -> Duration {
    dots.iter().map(TrailDot::ends).max().unwrap_or_default()
}

fn along_axis(from: i32, to: i32, fraction: f64) -> i32 {
    (f64::from(from) + (f64::from(to) - f64::from(from)) * fraction).round() as i32
}

fn dot_count(from: Point, to: Point) -> Option<usize> {
    let distance = (f64::from(to.x) - f64::from(from.x)).hypot(f64::from(to.y) - f64::from(from.y));
    let far = distance.is_finite() && distance >= MIN_DISTANCE;
    far.then(|| ((distance / SPACING).round() as usize).clamp(MIN_TRAIL_DOTS, MAX_TRAIL_DOTS))
}

fn fraction(index: usize, count: usize) -> f32 {
    (index + 1) as f32 / (count + 1) as f32
}

fn headward(index: usize, count: usize) -> f32 {
    index as f32 / (count - 1).max(1) as f32
}

fn sized(headward: f32, scale: f32) -> TrailDot {
    TrailDot {
        center: Point::default(),
        radius: scaled(lerp(TAIL_RADIUS, HEAD_RADIUS, headward), scale),
        peak: lerp(0.5, 1.0, headward),
        delay: Duration::ZERO,
        life: TRAIL_LIFE,
    }
}

pub(crate) fn paint_trail_dot(canvas: &mut Canvas<'_>, dot: &TrailDot, color: Rgb) {
    let center = dot.reach() as f32;
    for y in 0..canvas.height() {
        for x in 0..canvas.width() {
            let distance = (x as f32 + 0.5 - center).hypot(y as f32 + 0.5 - center);
            let body = Premul::of(color.mix(WHITE, 0.3), coverage(distance - dot.radius));
            let glow = Premul::of(color, 0.35 * (-(distance / (dot.radius * 1.6)).powi(2)).exp());
            canvas.set(x, y, body.over(glow).to_bgra());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::canvas::Bitmap;

    #[test]
    fn short_moves_leave_no_trail() {
        assert!(trail_dots(Point::new(10, 10), Point::new(30, 10), 1.0).is_empty());
        assert!(trail_dots(Point::new(10, 10), Point::new(10, 10), 1.0).is_empty());
    }

    #[test]
    fn trails_have_six_to_ten_dots_on_the_straight_path() {
        assert_eq!(trail_dots(Point::new(0, 0), Point::new(30, 0), 1.0).len(), 6);
        assert_eq!(trail_dots(Point::new(0, 0), Point::new(3000, 2000), 1.0).len(), 10);
        let dots = trail_dots(Point::new(100, 100), Point::new(500, 400), 1.0);
        assert!(dots.iter().all(|dot| ((dot.center.x - 100) * 3 - (dot.center.y - 100) * 4).abs() <= 4));
        assert!(dots.windows(2).all(|pair| pair[1].center.x > pair[0].center.x));
    }

    #[test]
    fn the_head_is_bigger_brighter_and_lasts_longest() {
        let dots = trail_dots(Point::new(0, 0), Point::new(400, 0), 1.0);
        let (tail, head) = (dots[0], dots[dots.len() - 1]);
        assert!(head.radius > tail.radius && head.peak > tail.peak);
        assert_eq!(head.life, TRAIL_LIFE);
        assert!(tail.life < head.life);
    }

    #[test]
    fn glide_dots_appear_along_the_eased_path_as_the_cursor_passes() {
        let (from, to) = (Point::new(0, 0), Point::new(400, 0));
        let duration = Duration::from_millis(120);
        let dots = glide_dots(from, to, duration, 1.0);
        assert_eq!(dots.len(), 8);
        assert!(dots.windows(2).all(|pair| pair[1].delay > pair[0].delay && pair[1].center.x > pair[0].center.x));
        assert!(dots.iter().all(|dot| dot.delay < duration && dot.center.y == 0));
        assert_eq!(dots[0].center, glide_point(from, to, 1.0 / 9.0));
        assert!(crate::overlay::easing::close(
            dots[3].alpha(dots[3].delay.saturating_sub(Duration::from_millis(1))),
            0.0
        ));
        assert!(crate::overlay::easing::close(dots[3].alpha(dots[3].delay), dots[3].peak));
        assert_eq!(trail_ends(&dots), dots[7].delay + TRAIL_LIFE);
        assert!(glide_dots(from, Point::new(10, 0), duration, 1.0).is_empty());
    }

    #[test]
    fn extreme_coordinates_do_not_overflow() {
        let (corner, opposite) = (Point::new(i32::MIN, i32::MIN), Point::new(i32::MAX, i32::MAX));
        let dots = trail_dots(corner, opposite, 1.0);
        assert_eq!(dots.len(), MAX_TRAIL_DOTS);
        assert!(dots.windows(2).all(|pair| pair[1].center.x > pair[0].center.x));
        assert_eq!(glide_dots(opposite, corner, Duration::from_millis(100), 1.0).len(), MAX_TRAIL_DOTS);
    }

    #[test]
    fn dots_fade_out_within_their_life() {
        let dot = trail_dots(Point::new(0, 0), Point::new(400, 0), 1.0)[0];
        assert!(crate::overlay::easing::close(dot.alpha(Duration::ZERO), dot.peak));
        assert!(crate::overlay::easing::close(dot.alpha(dot.life), 0.0));
        assert!(crate::overlay::easing::close(dot.alpha(TRAIL_LIFE), 0.0));
    }

    #[test]
    fn a_dot_is_a_premultiplied_disc() {
        let dot = trail_dots(Point::new(0, 0), Point::new(400, 0), 1.0)[5];
        let side = (dot.reach() * 2) as usize;
        let mut bitmap = Bitmap::new(side, side);
        paint_trail_dot(&mut bitmap.canvas().unwrap(), &dot, Rgb::new(0xD9, 0x77, 0x57));
        assert_eq!(bitmap.pixel(side / 2, side / 2) >> 24, 255);
        assert_eq!(bitmap.pixel(0, 0), 0);
    }
}
