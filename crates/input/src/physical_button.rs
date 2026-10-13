use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_SWAPBUTTON};
use winpilot_core::protocol::MouseButton;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PhysicalButton {
    Left,
    Right,
    Middle,
}

impl PhysicalButton {
    pub(crate) fn current(button: MouseButton) -> Self {
        Self::of(button, buttons_swapped())
    }

    pub(crate) fn of(button: MouseButton, swapped: bool) -> Self {
        match (button, swapped) {
            (MouseButton::Left, false) | (MouseButton::Right, true) => Self::Left,
            (MouseButton::Right, false) | (MouseButton::Left, true) => Self::Right,
            (MouseButton::Middle, _) => Self::Middle,
        }
    }
}

fn buttons_swapped() -> bool {
    unsafe { GetSystemMetrics(SM_SWAPBUTTON) != 0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unswapped_buttons_map_straight_through() {
        assert_eq!(PhysicalButton::of(MouseButton::Left, false), PhysicalButton::Left);
        assert_eq!(PhysicalButton::of(MouseButton::Right, false), PhysicalButton::Right);
        assert_eq!(PhysicalButton::of(MouseButton::Middle, false), PhysicalButton::Middle);
    }

    #[test]
    fn swapped_buttons_keep_left_as_the_primary_button() {
        assert_eq!(PhysicalButton::of(MouseButton::Left, true), PhysicalButton::Right);
        assert_eq!(PhysicalButton::of(MouseButton::Right, true), PhysicalButton::Left);
        assert_eq!(PhysicalButton::of(MouseButton::Middle, true), PhysicalButton::Middle);
    }

    #[test]
    fn reading_the_system_mapping_does_not_panic() {
        let _button = PhysicalButton::current(MouseButton::Left);
    }
}
