use std::{thread, time::Duration};

use deckhand_core::{Point, Result};

use crate::{
    mouse::cursor_position,
    send::send,
    timed_moves::{move_plan, play},
    virtual_desktop::VirtualDesktop,
};

pub fn glide_to_guarded(target: Point, duration: Duration, guard: impl FnMut() -> Result<()>) -> Result<()> {
    let from = cursor_position()?;
    let desktop = VirtualDesktop::current()?;
    play(&move_plan(desktop, from, target, duration), thread::sleep, send, guard)
}

pub fn glide_to(target: Point, duration: Duration) -> Result<()> {
    glide_to_guarded(target, duration, || Ok(()))
}
