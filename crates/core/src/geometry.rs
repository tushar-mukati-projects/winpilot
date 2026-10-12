use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub(crate) const COORD_LIMIT: i32 = 100_000;

const SIZE_LIMIT: i32 = 2 * COORD_LIMIT;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    pub(crate) const fn is_plausible(self) -> bool {
        within_coord_limit(self.x) && within_coord_limit(self.y)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
#[schemars(
    description = "A screen rectangle in physical pixels of the virtual screen (negative left of or above the primary monitor)."
)]
pub struct Rect {
    #[schemars(description = "Left edge, physical pixels.")]
    pub x: i32,
    #[schemars(description = "Top edge, physical pixels.")]
    pub y: i32,
    #[schemars(description = "Width in physical pixels.")]
    pub width: i32,
    #[schemars(description = "Height in physical pixels.")]
    pub height: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self { x, y, width, height }
    }

    pub const fn from_edges(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self { x: left, y: top, width: right.saturating_sub(left), height: bottom.saturating_sub(top) }
    }

    pub const fn right(self) -> i32 {
        self.x.saturating_add(self.width)
    }

    pub const fn bottom(self) -> i32 {
        self.y.saturating_add(self.height)
    }

    pub const fn is_empty(self) -> bool {
        self.width <= 0 || self.height <= 0
    }

    pub const fn center(self) -> Point {
        Point::new(self.x.saturating_add(self.width / 2), self.y.saturating_add(self.height / 2))
    }

    pub const fn contains(self, point: Point) -> bool {
        point.x >= self.x && point.x < self.right() && point.y >= self.y && point.y < self.bottom()
    }

    pub fn intersection(self, other: Self) -> Option<Self> {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = self.right().min(other.right());
        let bottom = self.bottom().min(other.bottom());
        let overlap = Self::from_edges(left, top, right, bottom);
        (!overlap.is_empty()).then_some(overlap)
    }

    #[must_use]
    pub fn union(self, other: Self) -> Self {
        let left = self.x.min(other.x);
        let top = self.y.min(other.y);
        Self::from_edges(left, top, self.right().max(other.right()), self.bottom().max(other.bottom()))
    }

    pub fn area(self) -> i64 {
        if self.is_empty() { 0 } else { i64::from(self.width).saturating_mul(i64::from(self.height)) }
    }

    pub(crate) const fn is_plausible(self) -> bool {
        Point::new(self.x, self.y).is_plausible() && within_size_limit(self.width) && within_size_limit(self.height)
    }
}

const fn within_coord_limit(value: i32) -> bool {
    value >= -COORD_LIMIT && value <= COORD_LIMIT
}

const fn within_size_limit(value: i32) -> bool {
    value >= 0 && value <= SIZE_LIMIT
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ImageMapping {
    pub origin: Point,
    pub scale: f64,
}

impl ImageMapping {
    pub fn to_screen(self, image_point: Point) -> Point {
        Point::new(
            self.origin.x.saturating_add(scaled(image_point.x, self.scale)),
            self.origin.y.saturating_add(scaled(image_point.y, self.scale)),
        )
    }

    pub fn to_image(self, screen_point: Point) -> Point {
        Point::new(
            unscaled(screen_point.x, self.origin.x, self.scale),
            unscaled(screen_point.y, self.origin.y, self.scale),
        )
    }

    pub fn rect_to_image(self, rect: Rect) -> Rect {
        let top_left = self.to_image(Point::new(rect.x, rect.y));
        let bottom_right = self.to_image(Point::new(rect.right(), rect.bottom()));
        Rect::from_edges(top_left.x, top_left.y, bottom_right.x, bottom_right.y)
    }
}

fn scaled(value: i32, scale: f64) -> i32 {
    (f64::from(value) * scale).round() as i32
}

fn unscaled(value: i32, origin: i32, scale: f64) -> i32 {
    ((f64::from(value) - f64::from(origin)) / scale).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intersection_of_overlapping_rects() {
        let a = Rect::new(0, 0, 100, 100);
        let b = Rect::new(50, 60, 100, 100);
        assert_eq!(a.intersection(b), Some(Rect::new(50, 60, 50, 40)));
    }

    #[test]
    fn union_spans_both_rects() {
        let a = Rect::new(10, 20, 30, 10);
        let b = Rect::new(50, 15, 10, 30);
        assert_eq!(a.union(b), Rect::new(10, 15, 50, 30));
        assert_eq!(a.union(a), a);
    }

    #[test]
    fn intersection_of_touching_rects_is_none() {
        let a = Rect::new(0, 0, 10, 10);
        let b = Rect::new(10, 0, 10, 10);
        assert_eq!(a.intersection(b), None);
    }

    #[test]
    fn contains_excludes_right_and_bottom_edges() {
        let rect = Rect::new(10, 10, 5, 5);
        assert!(rect.contains(Point::new(10, 10)));
        assert!(rect.contains(Point::new(14, 14)));
        assert!(!rect.contains(Point::new(15, 10)));
        assert!(!rect.contains(Point::new(10, 15)));
    }

    #[test]
    fn mapping_round_trips() {
        let mapping = ImageMapping { origin: Point::new(-1920, 0), scale: 2.0 };
        let screen = mapping.to_screen(Point::new(640, 400));
        assert_eq!(screen, Point::new(-640, 800));
        assert_eq!(mapping.to_image(screen), Point::new(640, 400));
    }

    #[test]
    fn edges_and_centers_saturate_instead_of_overflowing() {
        assert_eq!(Rect::new(i32::MAX, 0, 10, 10).right(), i32::MAX);
        assert_eq!(Rect::new(0, i32::MIN, 10, -10).bottom(), i32::MIN);
        assert_eq!(Rect::new(i32::MAX - 1, 0, 10, 10).center(), Point::new(i32::MAX, 5));
        assert_eq!(Rect::from_edges(i32::MIN, 0, i32::MAX, 10), Rect::new(i32::MIN, 0, i32::MAX, 10));
        assert_eq!(Rect::new(0, 0, i32::MAX, i32::MAX).area(), i64::from(i32::MAX) * i64::from(i32::MAX));
    }

    #[test]
    fn intersection_with_a_far_region_does_not_overflow() {
        let far = Rect::new(i32::MAX - 5, 0, 100, 100);
        assert_eq!(far.intersection(Rect::new(0, 0, 1920, 1080)), None);
        assert_eq!(far.intersection(far), Some(Rect::new(i32::MAX - 5, 0, 5, 100)));
    }

    #[test]
    fn mapping_saturates_on_extreme_points() {
        let mapping = ImageMapping { origin: Point::new(1920, 0), scale: 1.0 };
        assert_eq!(mapping.to_screen(Point::new(i32::MAX, 0)), Point::new(i32::MAX, 0));
        assert_eq!(mapping.to_image(Point::new(i32::MIN, 0)), Point::new(i32::MIN, 0));
        let doubled = ImageMapping { origin: Point::new(0, 0), scale: 2.0 };
        assert_eq!(doubled.to_screen(Point::new(1_100_000_000, 0)), Point::new(i32::MAX, 0));
        let zero = ImageMapping { origin: Point::new(0, 0), scale: 0.0 };
        assert_eq!(zero.to_image(Point::new(5, 0)), Point::new(i32::MAX, 0));
        let huge = mapping.rect_to_image(Rect::new(i32::MAX - 1, 0, i32::MAX, 10));
        assert!(huge.width >= 0);
    }

    #[test]
    fn mapping_round_trips_at_fractional_scale() {
        let mapping = ImageMapping { origin: Point::new(-1920, -7), scale: 1.5 };
        for value in -50..50 {
            let image = Point::new(value, value);
            assert_eq!(mapping.to_image(mapping.to_screen(image)), image);
        }
    }

    #[test]
    fn plausible_points_stay_within_the_coordinate_limit() {
        assert!(Point::new(0, 0).is_plausible());
        assert!(Point::new(-COORD_LIMIT, COORD_LIMIT).is_plausible());
        assert!(!Point::new(COORD_LIMIT + 1, 0).is_plausible());
        assert!(!Point::new(0, -COORD_LIMIT - 1).is_plausible());
        assert!(!Point::new(i32::MIN, i32::MAX).is_plausible());
    }

    #[test]
    fn plausible_rects_have_bounded_origin_and_size() {
        assert!(Rect::new(-1920, 0, 3840, 1080).is_plausible());
        assert!(Rect::new(0, 0, 0, 0).is_plausible());
        assert!(Rect::new(-COORD_LIMIT, -COORD_LIMIT, 2 * COORD_LIMIT, 2 * COORD_LIMIT).is_plausible());
        assert!(!Rect::new(0, 0, -1, 10).is_plausible());
        assert!(!Rect::new(0, 0, 10, 2 * COORD_LIMIT + 1).is_plausible());
        assert!(!Rect::new(COORD_LIMIT + 1, 0, 10, 10).is_plausible());
        assert!(!Rect::new(i32::MAX, 0, 10, 10).is_plausible());
    }
}
