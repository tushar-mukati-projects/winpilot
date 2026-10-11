use deckhand_core::{Result, TargetSpec, keys::parse_keys};

use super::{
    done::{Done, via_focus_click},
    focus::keyboard_target,
    settle,
};
use crate::{call_scope::CallScope, engine::Engine};

pub fn step(engine: &Engine, keys: &str, target: Option<&TargetSpec>, repeat: u32, call: &CallScope) -> Result<Done> {
    let chords = parse_keys(keys)?;
    let focused = keyboard_target(engine, target, call)?;
    for _ in 0..repeat {
        engine.guard.check_running(call)?;
        chords.iter().try_for_each(deckhand_input::press_chord)?;
    }
    settle(engine, &focused.window, call);
    Ok(Done::by(via_focus_click("keys", focused.clicked)))
}
