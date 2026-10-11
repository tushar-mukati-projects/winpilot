use deckhand_core::Rect;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Perimeter {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BorderPoint {
    pub(crate) along: f32,
    pub(crate) outside: f32,
}

impl Perimeter {
    pub(crate) fn of(rect: Rect) -> Self {
        Self { left: rect.x as f32, top: rect.y as f32, right: rect.right() as f32, bottom: rect.bottom() as f32 }
    }

    pub(crate) fn width(&self) -> f32 {
        (self.right - self.left).max(0.0)
    }

    pub(crate) fn height(&self) -> f32 {
        (self.bottom - self.top).max(0.0)
    }

    pub(crate) fn length(&self) -> f32 {
        2.0 * (self.width() + self.height())
    }

    pub(crate) fn along_top(&self, x: f32) -> f32 {
        (x - self.left).clamp(0.0, self.width())
    }

    pub(crate) fn along_right(&self, y: f32) -> f32 {
        self.width() + (y - self.top).clamp(0.0, self.height())
    }

    pub(crate) fn along_bottom(&self, x: f32) -> f32 {
        self.width() + self.height() + (self.right - x).clamp(0.0, self.width())
    }

    pub(crate) fn along_left(&self, y: f32) -> f32 {
        2.0 * self.width() + self.height() + (self.bottom - y).clamp(0.0, self.height())
    }

    pub(crate) fn horizontal_gap(&self, x: f32) -> f32 {
        (self.left - x).max(x - self.right).max(0.0)
    }

    pub(crate) fn vertical_gap(&self, y: f32) -> f32 {
        (self.top - y).max(y - self.bottom).max(0.0)
    }

    #[cfg(test)]
    pub(crate) fn locate(&self, x: f32, y: f32) -> Option<BorderPoint> {
        let (dx, dy) = (self.horizontal_gap(x), self.vertical_gap(y));
        let along = match (dx > 0.0, dy > 0.0) {
            (false, false) => return None,
            (_, true) if y < self.top => self.along_top(x),
            (_, true) => self.along_bottom(x),
            (true, false) if x < self.left => self.along_left(y),
            (true, false) => self.along_right(y),
        };
        Some(BorderPoint { along: along % self.length().max(1.0), outside: dx.hypot(dy) })
    }
}

pub(crate) fn wrapped_distance(from: f32, to: f32, length: f32) -> f32 {
    if length <= 0.0 {
        return 0.0;
    }
    let forward = (to - from).rem_euclid(length);
    if forward > length / 2.0 { forward - length } else { forward }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> Perimeter {
        Perimeter::of(Rect::new(100, 50, 200, 100))
    }

    #[test]
    fn perimeter_runs_clockwise_from_the_top_left() {
        let frame = frame();
        assert!(crate::easing::close(frame.length(), 600.0));
        assert_eq!(frame.locate(150.5, 49.5).map(|p| p.along), Some(50.5));
        assert_eq!(frame.locate(300.5, 100.5).map(|p| p.along), Some(250.5));
        assert_eq!(frame.locate(250.5, 150.5).map(|p| p.along), Some(349.5));
        assert_eq!(frame.locate(99.5, 100.5).map(|p| p.along), Some(549.5));
    }

    #[test]
    fn inside_points_are_not_on_the_border() {
        assert_eq!(frame().locate(150.5, 100.5), None);
        assert_eq!(frame().locate(100.5, 50.5), None);
    }

    #[test]
    fn corners_measure_euclidean_distance() {
        let corner = frame().locate(97.0, 46.0).unwrap();
        assert!(crate::easing::close(corner.outside, 5.0));
        assert!(crate::easing::close(corner.along, 0.0));
    }

    #[test]
    fn wrapped_distances_take_the_short_way_round() {
        assert!(crate::easing::close(wrapped_distance(590.0, 10.0, 600.0), 20.0));
        assert!(crate::easing::close(wrapped_distance(10.0, 590.0, 600.0), -20.0));
        assert!(crate::easing::close(wrapped_distance(0.0, 5.0, 0.0), 0.0));
    }
}
