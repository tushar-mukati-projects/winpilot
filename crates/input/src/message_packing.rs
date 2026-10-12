use deckhand_core::{
    Point,
    protocol::{Direction, MouseButton},
};
use windows::Win32::UI::WindowsAndMessaging::{
    WHEEL_DELTA, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDBLCLK, WM_MBUTTONDOWN, WM_MBUTTONUP,
    WM_MOUSEHWHEEL, WM_MOUSEWHEEL, WM_RBUTTONDBLCLK, WM_RBUTTONDOWN, WM_RBUTTONUP,
};

const HELD_LEFT: usize = 0x0001;
const HELD_RIGHT: usize = 0x0002;
const HELD_MIDDLE: usize = 0x0010;
const REPEAT_ONCE: u32 = 1;
const SCAN_SHIFT: u32 = 16;
const EXTENDED_BIT: u32 = 1 << 24;
const RELEASED_BITS: u32 = 0xC000_0000;
const HIGH_WORD_SHIFT: u32 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ButtonMessages {
    pub(crate) down: u32,
    pub(crate) up: u32,
    pub(crate) double: u32,
    pub(crate) held: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KeyCode {
    pub(crate) vk: u16,
    pub(crate) scan: u16,
    pub(crate) extended: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Wheel {
    pub(crate) message: u32,
    pub(crate) delta: i16,
}

pub(crate) const fn button_messages(button: MouseButton) -> ButtonMessages {
    match button {
        MouseButton::Left => {
            ButtonMessages { down: WM_LBUTTONDOWN, up: WM_LBUTTONUP, double: WM_LBUTTONDBLCLK, held: HELD_LEFT }
        }
        MouseButton::Right => {
            ButtonMessages { down: WM_RBUTTONDOWN, up: WM_RBUTTONUP, double: WM_RBUTTONDBLCLK, held: HELD_RIGHT }
        }
        MouseButton::Middle => {
            ButtonMessages { down: WM_MBUTTONDOWN, up: WM_MBUTTONUP, double: WM_MBUTTONDBLCLK, held: HELD_MIDDLE }
        }
    }
}

pub(crate) fn point_lparam(point: Point) -> isize {
    let x = u32::from(point.x as i16 as u16);
    let y = u32::from(point.y as i16 as u16);
    (x | (y << HIGH_WORD_SHIFT)) as isize
}

pub(crate) fn key_lparam(code: KeyCode, released: bool) -> isize {
    let mut value = REPEAT_ONCE | (u32::from(code.scan) << SCAN_SHIFT);
    if code.extended {
        value |= EXTENDED_BIT;
    }
    if released {
        value |= RELEASED_BITS;
    }
    value as isize
}

pub(crate) const fn char_lparam() -> isize {
    REPEAT_ONCE as isize
}

pub(crate) const fn wheel(direction: Direction) -> Wheel {
    let notch = WHEEL_DELTA as i16;
    match direction {
        Direction::Up => Wheel { message: WM_MOUSEWHEEL, delta: notch },
        Direction::Down => Wheel { message: WM_MOUSEWHEEL, delta: -notch },
        Direction::Right => Wheel { message: WM_MOUSEHWHEEL, delta: notch },
        Direction::Left => Wheel { message: WM_MOUSEHWHEEL, delta: -notch },
    }
}

pub(crate) fn wheel_wparam(delta: i16) -> usize {
    usize::from(delta as u16) << HIGH_WORD_SHIFT
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_pack_into_low_and_high_words() {
        assert_eq!(point_lparam(Point::new(0, 0)), 0);
        assert_eq!(point_lparam(Point::new(10, 20)), 0x0014_000A);
        assert_eq!(point_lparam(Point::new(1919, 1079)), 0x0437_077F);
    }

    #[test]
    fn negative_coordinates_keep_their_sign_in_each_word() {
        let packed = point_lparam(Point::new(-8, -30)) as u32;
        assert_eq!((packed & 0xFFFF) as u16 as i16, -8);
        assert_eq!((packed >> 16) as u16 as i16, -30);
    }

    #[test]
    fn key_lparam_carries_repeat_scan_extended_and_release_bits() {
        let end = KeyCode { vk: 0x23, scan: 0x4F, extended: true };
        assert_eq!(key_lparam(end, false), 0x014F_0001);
        assert_eq!(key_lparam(end, true), 0xC14F_0001_u32 as isize);
        let back = KeyCode { vk: 0x08, scan: 0x0E, extended: false };
        assert_eq!(key_lparam(back, false), 0x000E_0001);
        assert_eq!(key_lparam(back, true), 0xC00E_0001_u32 as isize);
    }

    #[test]
    fn characters_are_one_repeat() {
        assert_eq!(char_lparam(), 1);
    }

    #[test]
    fn each_button_has_its_own_messages_and_held_flag() {
        let left = button_messages(MouseButton::Left);
        assert_eq!((left.down, left.up, left.double, left.held), (0x0201, 0x0202, 0x0203, 0x0001));
        let right = button_messages(MouseButton::Right);
        assert_eq!((right.down, right.up, right.double, right.held), (0x0204, 0x0205, 0x0206, 0x0002));
        let middle = button_messages(MouseButton::Middle);
        assert_eq!((middle.down, middle.up, middle.double, middle.held), (0x0207, 0x0208, 0x0209, 0x0010));
    }

    #[test]
    fn wheel_directions_pick_the_message_and_sign() {
        assert_eq!(wheel(Direction::Up), Wheel { message: 0x020A, delta: 120 });
        assert_eq!(wheel(Direction::Down), Wheel { message: 0x020A, delta: -120 });
        assert_eq!(wheel(Direction::Right), Wheel { message: 0x020E, delta: 120 });
        assert_eq!(wheel(Direction::Left), Wheel { message: 0x020E, delta: -120 });
    }

    #[test]
    fn the_wheel_delta_sits_in_the_high_word() {
        assert_eq!(wheel_wparam(120), 0x0078_0000);
        assert_eq!(wheel_wparam(-120), 0xFF88_0000);
    }
}
