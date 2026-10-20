use std::{thread, time::Duration};

use winpilot_core::{Point, Result};

use crate::{
    mouse::cursor_position,
    send::send,
    timed_moves::{move_plan, play},
    virtual_screen::current_screen,
};

pub fn glide_to_guarded(target: Point, duration: Duration, guard: impl FnMut() -> Result<()>) -> Result<()> {
    let from = cursor_position()?;
    let screen = current_screen()?;
    play(&move_plan(screen, from, target, duration), thread::sleep, send, guard)
}
