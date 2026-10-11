use std::time::{Duration, Instant};

use deckhand_core::{
    Point, Result,
    keys::{Chord, Modifiers},
    protocol::{Direction, MouseButton},
};

pub const INJECTION_MARK: usize = 0x4445_434B;

pub fn move_to(point: Point) -> Result<()> {
    todo!("{point:?}")
}

pub fn click(point: Point, button: MouseButton, count: u8, modifiers: Modifiers) -> Result<()> {
    todo!("{point:?} {button:?} {count} {modifiers:?}")
}

pub fn drag(from: Point, to: Point, button: MouseButton) -> Result<()> {
    todo!("{from:?} {to:?} {button:?}")
}

pub fn scroll(point: Point, direction: Direction, notches: u32) -> Result<()> {
    todo!("{point:?} {direction:?} {notches}")
}

pub fn press_chord(chord: &Chord) -> Result<()> {
    todo!("{chord:?}")
}

pub fn type_text(text: &str) -> Result<()> {
    todo!("{text}")
}

pub fn tap_alt() -> Result<()> {
    todo!()
}

pub fn cursor_position() -> Result<Point> {
    todo!()
}

pub struct ActivityMonitor {
    _private: (),
}

impl ActivityMonitor {
    pub fn spawn() -> Result<Self> {
        todo!()
    }

    pub fn last_physical_input(&self) -> Option<Instant> {
        todo!()
    }

    pub fn idle_for(&self) -> Option<Duration> {
        self.last_physical_input().map(|at| at.elapsed())
    }
}

pub struct HotkeyListener {
    _private: (),
}

impl HotkeyListener {
    pub fn spawn(chord: Chord, on_press: impl Fn() + Send + 'static) -> Result<Self> {
        let _ = on_press;
        todo!("{chord:?}")
    }

    pub fn is_registered(&self) -> bool {
        todo!()
    }
}
