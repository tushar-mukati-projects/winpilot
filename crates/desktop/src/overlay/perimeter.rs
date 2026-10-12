use deckhand_core::Rect;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Perimeter {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
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
        assert!(crate::overlay::easing::close(frame.length(), 600.0));
        assert!(crate::overlay::easing::close(frame.along_top(150.5), 50.5));
        assert!(crate::overlay::easing::close(frame.along_right(100.5), 250.5));
        assert!(crate::overlay::easing::close(frame.along_bottom(250.5), 349.5));
        assert!(crate::overlay::easing::close(frame.along_left(100.5), 549.5));
    }

    #[test]
    fn gaps_are_zero_inside_and_grow_outside() {
        let frame = frame();
        assert!(crate::overlay::easing::close(frame.horizontal_gap(150.5), 0.0));
        assert!(crate::overlay::easing::close(frame.vertical_gap(100.5), 0.0));
        assert!(crate::overlay::easing::close(frame.horizontal_gap(97.0), 3.0));
        assert!(crate::overlay::easing::close(frame.vertical_gap(46.0), 4.0));
    }

    #[test]
    fn wrapped_distances_take_the_short_way_round() {
        assert!(crate::overlay::easing::close(wrapped_distance(590.0, 10.0, 600.0), 20.0));
        assert!(crate::overlay::easing::close(wrapped_distance(10.0, 590.0, 600.0), -20.0));
        assert!(crate::overlay::easing::close(wrapped_distance(0.0, 5.0, 0.0), 0.0));
    }
}
