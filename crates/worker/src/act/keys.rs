use deckhand_core::{
    Result,
    keys::{Chord, parse_keys},
    protocol::KeyAction,
};

use super::{
    done::{Done, via_focus_click},
    focus::keyboard_target,
    key_labels::key_chips,
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
    match request.action {
        KeyAction::Up => release(engine, &chords, call),
        KeyAction::Down => {
            send_to_focus(engine, target, &chords, call, || hold(&chords, call).map(|()| Some(HELD.to_string())))
        }
        KeyAction::Press => {
            send_to_focus(engine, target, &chords, call, || press(engine, &chords, request.repeat, call).map(|()| None))
        }
    }
}

fn send_to_focus(
    engine: &Engine,
    target: Option<&Prepared>,
    chords: &[Chord],
    call: &CallScope,
    send: impl FnOnce() -> Result<Option<String>>,
) -> Result<Done> {
    let focused = keyboard_target(engine, target, call)?;
    if let Some(element) = &focused.element {
        overlay::focus_field(engine, element.rect);
    }
    overlay::key_chips(engine, &focused.window, key_chips(chords));
    let note = send()?;
    settle(engine, &focused.window, call);
    Ok(Done::by(via_focus_click("keys", focused.clicked)).with_note(note))
}

fn press(engine: &Engine, chords: &[Chord], repeat: u32, call: &CallScope) -> Result<()> {
    for _ in 0..repeat {
        for chord in chords {
            engine.guard.check_running(call)?;
            deckhand_input::press_chord(chord)?;
        }
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
