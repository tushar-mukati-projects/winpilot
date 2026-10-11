use std::{thread, time::Duration};

use deckhand_core::{Point, Result, protocol::MouseButton};

use crate::{
    mouse::{ButtonTransition, button_event, move_event},
    send::send,
    virtual_desktop::VirtualDesktop,
};

const DRAG_STEPS: u32 = 12;
const DRAG_STEP_DELAY: Duration = Duration::from_millis(8);

pub fn drag(from: Point, to: Point, button: MouseButton) -> Result<()> {
    let desktop = VirtualDesktop::current()?;
    let outcome = perform_drag(desktop, from, to, button);
    if outcome.is_err() {
        let _ = send(&[button_event(button, ButtonTransition::Release)]);
    }
    outcome
}

fn perform_drag(desktop: VirtualDesktop, from: Point, to: Point, button: MouseButton) -> Result<()> {
    send(&[move_event(desktop, from), button_event(button, ButtonTransition::Press)])?;
    for point in drag_path(from, to) {
        thread::sleep(DRAG_STEP_DELAY);
        send(&[move_event(desktop, point)])?;
    }
    thread::sleep(DRAG_STEP_DELAY);
    send(&[move_event(desktop, to), button_event(button, ButtonTransition::Release)])
}

fn drag_path(from: Point, to: Point) -> Vec<Point> {
    let segments = f64::from(DRAG_STEPS + 1);
    (1..=DRAG_STEPS).map(|step| interpolate(from, to, f64::from(step) / segments)).collect()
}

fn interpolate(from: Point, to: Point, fraction: f64) -> Point {
    Point::new(lerp(from.x, to.x, fraction), lerp(from.y, to.y, fraction))
}

fn lerp(start: i32, end: i32, fraction: f64) -> i32 {
    (f64::from(start) + (f64::from(end) - f64::from(start)) * fraction).round() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_has_twelve_points_strictly_between_the_ends() {
        let path = drag_path(Point::new(0, 0), Point::new(130, 260));
        assert_eq!(path.len(), 12);
        assert_eq!(path[0], Point::new(10, 20));
        assert_eq!(path[11], Point::new(120, 240));
        assert!(path.windows(2).all(|pair| pair[0].x < pair[1].x && pair[0].y < pair[1].y));
    }

    #[test]
    fn path_runs_backwards_across_negative_coordinates() {
        let path = drag_path(Point::new(100, 0), Point::new(-1720, -1300));
        assert_eq!(path[0], Point::new(-40, -100));
        assert_eq!(path[11], Point::new(-1580, -1200));
    }

    #[test]
    fn zero_length_drag_stays_put() {
        let path = drag_path(Point::new(5, 5), Point::new(5, 5));
        assert!(path.iter().all(|point| *point == Point::new(5, 5)));
    }

    #[test]
    fn extreme_coordinates_do_not_overflow() {
        let path = drag_path(Point::new(i32::MIN, i32::MAX), Point::new(i32::MAX, i32::MIN));
        assert_eq!(path.len(), 12);
        assert!(path[0].x < 0 && path[11].x > 0);
    }
}
