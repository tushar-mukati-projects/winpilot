use deckhand_core::{Result, TargetSpec, keys::parse_keys};

use super::{
    done::{Done, via_focus_click},
    focus::keyboard_target,
    key_labels::chord_labels,
    settle,
};
use crate::{call_scope::CallScope, engine::Engine, overlay};

pub fn step(engine: &Engine, keys: &str, target: Option<&TargetSpec>, repeat: u32, call: &CallScope) -> Result<Done> {
    let chords = parse_keys(keys)?;
    let focused = keyboard_target(engine, target, call)?;
    if let Some(element) = &focused.element {
        overlay::focus_field(engine, element.rect);
    }
    overlay::keystrokes(engine, &focused.window, chord_labels(&chords));
    for _ in 0..repeat {
        engine.guard.check_running(call)?;
        chords.iter().try_for_each(deckhand_input::press_chord)?;
    }
    settle(engine, &focused.window, call);
    Ok(Done::by(via_focus_click("keys", focused.clicked)))
}
