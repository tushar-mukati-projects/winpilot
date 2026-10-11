use deckhand_core::{
    Result,
    keys::{Chord, parse_keys},
    protocol::KeyAction,
};

use super::{
    done::{Done, via_focus_click},
    focus::keyboard_target,
    key_labels::chord_labels,
    settle,
    target::Prepared,
};
use crate::{call_scope::CallScope, engine::Engine, overlay};

const HELD: &str = "held until a key up step or the end of the call";

pub struct KeyRequest<'a> {
    pub keys: &'a str,
    pub repeat: u32,
    pub action: KeyAction,
}

pub fn step(engine: &Engine, target: Option<&Prepared>, request: &KeyRequest<'_>, call: &CallScope) -> Result<Done> {
    let chords = parse_keys(request.keys)?;
    if request.action == KeyAction::Up {
        return release(engine, &chords, call);
    }
    let focused = keyboard_target(engine, target, call)?;
    if let Some(element) = &focused.element {
        overlay::focus_field(engine, element.rect);
    }
    overlay::keystrokes(engine, &focused.window, chord_labels(&chords));
    let note = match request.action {
        KeyAction::Down => hold(&chords, call).map(|()| Some(HELD.to_string()))?,
        KeyAction::Press | KeyAction::Up => press(engine, &chords, request.repeat, call).map(|()| None)?,
    };
    settle(engine, &focused.window, call);
    Ok(Done::by(via_focus_click("keys", focused.clicked)).with_note(note))
}

fn press(engine: &Engine, chords: &[Chord], repeat: u32, call: &CallScope) -> Result<()> {
    for _ in 0..repeat {
        engine.guard.check_running(call)?;
        chords.iter().try_for_each(deckhand_input::press_chord)?;
    }
    Ok(())
}

fn hold(chords: &[Chord], call: &CallScope) -> Result<()> {
    for chord in chords {
        deckhand_input::hold_chord(chord)?;
        call.note_held_keys(*chord);
    }
    Ok(())
}

fn release(engine: &Engine, chords: &[Chord], call: &CallScope) -> Result<Done> {
    engine.guard.check_running(call)?;
    for chord in chords {
        deckhand_input::release_chord(chord)?;
        call.note_released_keys(*chord);
    }
    Ok(Done::by("keys"))
}
