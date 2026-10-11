#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RoundedBox {
    center_x: f32,
    center_y: f32,
    half_width: f32,
    half_height: f32,
    radius: f32,
}

impl RoundedBox {
    pub(crate) fn new(left: f32, top: f32, width: f32, height: f32, radius: f32) -> Self {
        let (half_width, half_height) = (width.max(0.0) / 2.0, height.max(0.0) / 2.0);
        Self {
            center_x: left + half_width,
            center_y: top + half_height,
            half_width,
            half_height,
            radius: radius.clamp(0.0, half_width.min(half_height)),
        }
    }

    pub(crate) fn distance(&self, x: f32, y: f32) -> f32 {
        let qx = (x - self.center_x).abs() - self.half_width + self.radius;
        let qy = (y - self.center_y).abs() - self.half_height + self.radius;
        let outside = qx.max(0.0).hypot(qy.max(0.0));
        let inside = qx.max(qy).min(0.0);
        outside + inside - self.radius
    }

    pub(crate) fn shifted(self, dx: f32, dy: f32) -> Self {
        Self { center_x: self.center_x + dx, center_y: self.center_y + dy, ..self }
    }
}

pub(crate) fn coverage(distance: f32) -> f32 {
    (0.5 - distance).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_is_negative_inside_and_positive_outside() {
        let pill = RoundedBox::new(0.0, 0.0, 100.0, 20.0, 10.0);
        assert!(crate::overlay::easing::close(pill.distance(50.0, 10.0), -10.0));
        assert!(crate::overlay::easing::close(pill.distance(50.0, -5.0), 5.0));
        assert!((pill.distance(-3.0, 10.0) - 3.0).abs() < 1e-5);
    }

    #[test]
    fn corners_are_rounded() {
        let square = RoundedBox::new(0.0, 0.0, 40.0, 40.0, 10.0);
        assert!(square.distance(1.0, 1.0) > 0.0);
        assert!(square.distance(10.0, 10.0) < 0.0);
    }

    #[test]
    fn coverage_antialiases_the_edge() {
        assert!(crate::overlay::easing::close(coverage(-2.0), 1.0));
        assert!(crate::overlay::easing::close(coverage(0.0), 0.5));
        assert!(crate::overlay::easing::close(coverage(2.0), 0.0));
    }
}
