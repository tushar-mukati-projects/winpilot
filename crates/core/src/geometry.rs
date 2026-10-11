use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize, JsonSchema)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self { x, y, width, height }
    }

    pub const fn from_edges(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self { x: left, y: top, width: right - left, height: bottom - top }
    }

    pub const fn right(self) -> i32 {
        self.x + self.width
    }

    pub const fn bottom(self) -> i32 {
        self.y + self.height
    }

    pub const fn is_empty(self) -> bool {
        self.width <= 0 || self.height <= 0
    }

    pub const fn center(self) -> Point {
        Point::new(self.x + self.width / 2, self.y + self.height / 2)
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

    pub fn area(self) -> i64 {
        if self.is_empty() { 0 } else { i64::from(self.width) * i64::from(self.height) }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ImageMapping {
    pub origin: Point,
    pub scale: f64,
}

impl ImageMapping {
    pub fn to_screen(self, image_point: Point) -> Point {
        Point::new(
            self.origin.x + (f64::from(image_point.x) * self.scale).round() as i32,
            self.origin.y + (f64::from(image_point.y) * self.scale).round() as i32,
        )
    }

    pub fn to_image(self, screen_point: Point) -> Point {
        Point::new(
            (f64::from(screen_point.x - self.origin.x) / self.scale).round() as i32,
            (f64::from(screen_point.y - self.origin.y) / self.scale).round() as i32,
        )
    }

    pub fn rect_to_image(self, rect: Rect) -> Rect {
        let top_left = self.to_image(Point::new(rect.x, rect.y));
        let bottom_right = self.to_image(Point::new(rect.right(), rect.bottom()));
        Rect::from_edges(top_left.x, top_left.y, bottom_right.x, bottom_right.y)
    }
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
}
