use winpilot_core::{Point, keys::Chord, protocol::MouseButton};

use super::CallScope;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HeldButton {
    pub(crate) button: MouseButton,
    pub(crate) pressed_at: Point,
}

impl CallScope {
    pub(crate) fn note_held(&self, button: MouseButton, pressed_at: Point) {
        let mut held = self.held.lock();
        if held.iter().all(|held| held.button != button) {
            held.push(HeldButton { button, pressed_at });
        }
    }

    pub(crate) fn note_released(&self, button: MouseButton) {
        self.held.lock().retain(|held| held.button != button);
    }

    pub(crate) fn take_held(&self) -> Vec<HeldButton> {
        std::mem::take(&mut *self.held.lock())
    }

    pub(crate) fn held_buttons(&self) -> Vec<MouseButton> {
        self.held.lock().iter().map(|held| held.button).collect()
    }

    pub(crate) fn note_held_keys(&self, chord: Chord) {
        let mut held = self.held_keys.lock();
        if !held.contains(&chord) {
            held.push(chord);
        }
    }

    pub(crate) fn note_released_keys(&self, chord: Chord) {
        self.held_keys.lock().retain(|held| *held != chord);
    }

    pub(crate) fn take_held_keys(&self) -> Vec<Chord> {
        std::mem::take(&mut *self.held_keys.lock())
    }

    pub(crate) fn held_keys(&self) -> Vec<Chord> {
        self.held_keys.lock().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn held_buttons_keep_their_first_press_point_until_released_or_taken() {
        let scope = CallScope::relaxed();
        scope.note_held(MouseButton::Left, Point::new(10, 20));
        scope.note_held(MouseButton::Left, Point::new(99, 99));
        scope.note_held(MouseButton::Right, Point::new(5, 5));
        assert_eq!(scope.held_buttons(), vec![MouseButton::Left, MouseButton::Right]);
        scope.note_released(MouseButton::Right);
        let left = HeldButton { button: MouseButton::Left, pressed_at: Point::new(10, 20) };
        assert_eq!(scope.take_held(), vec![left]);
        assert!(scope.take_held().is_empty() && scope.held_buttons().is_empty());
    }

    #[test]
    fn held_keys_are_tracked_until_released_or_taken() {
        let scope = CallScope::relaxed();
        let shift = winpilot_core::keys::parse_chord("shift").unwrap();
        let ctrl_a = winpilot_core::keys::parse_chord("ctrl+a").unwrap();
        scope.note_held_keys(shift);
        scope.note_held_keys(shift);
        scope.note_held_keys(ctrl_a);
        scope.note_released_keys(ctrl_a);
        assert_eq!(scope.held_keys(), vec![shift]);
        assert_eq!(scope.take_held_keys(), vec![shift]);
        assert!(scope.take_held_keys().is_empty());
    }
}
