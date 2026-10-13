use winpilot_core::{Element, Point, Rect, WindowInfo};

pub(crate) fn visible_part(element: &Element, window: &WindowInfo) -> Option<Rect> {
    if !element.is_visible() {
        return None;
    }
    element.rect.intersection(window.rect)
}

pub(crate) fn clickable_point(element: &Element, window: &WindowInfo) -> Option<Point> {
    visible_part(element, window).map(Rect::center)
}

#[cfg(test)]
mod tests {
    use winpilot_core::Role;

    use super::*;
    use crate::test_fixture::{element, window};

    fn placed(rect: Rect) -> Element {
        Element { rect, ..element(1, 1, Role::Button, "OK") }
    }

    #[test]
    fn the_point_is_the_centre_of_the_part_inside_the_window() {
        let frame = window(1, "app.exe", "App", Rect::new(0, 0, 100, 100));
        assert_eq!(clickable_point(&placed(Rect::new(10, 10, 20, 20)), &frame), Some(Point::new(20, 20)));
        assert_eq!(visible_part(&placed(Rect::new(90, 90, 20, 20)), &frame), Some(Rect::new(90, 90, 10, 10)));
    }

    #[test]
    fn elements_outside_the_window_have_no_point() {
        let frame = window(1, "app.exe", "App", Rect::new(0, 0, 100, 100));
        assert_eq!(clickable_point(&placed(Rect::new(200, 200, 20, 20)), &frame), None);
        let mut hidden = placed(Rect::new(10, 10, 20, 20));
        hidden.state.offscreen = true;
        assert_eq!(clickable_point(&hidden, &frame), None);
    }
}
