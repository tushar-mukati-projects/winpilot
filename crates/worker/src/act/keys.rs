use deckhand_core::{
    Error, Result,
    keys::{Chord, parse_keys},
    protocol::KeyAction,
};

use super::{
    done::{Done, via_focus_click},
    focus::keyboard_target,
    key_labels::key_chips,
    message_keys,
    mode::Mode,
    settle,
    target::Prepared,
};
use crate::{call_scope::CallScope, engine::Engine, host_terminal, overlay};

const HELD: &str = "held until a key up step or the end of the call";

pub(crate) struct KeyRequest<'a> {
    pub(crate) keys: &'a str,
    pub(crate) repeat: u32,
    pub(crate) action: KeyAction,
}

pub(crate) fn step(
    engine: &Engine,
    target: Option<&Prepared>,
    request: &KeyRequest<'_>,
    mode: Mode,
    call: &CallScope,
) -> Result<Done> {
    let chords = parse_keys(request.keys)?;
    if let Some(prepared) = target {
        host_terminal::refuse_keys(engine, &prepared.window)?;
    }
    if mode == Mode::Background {
        return press_in_background(engine, target, &chords, request, call);
    }
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

fn press_in_background(
    engine: &Engine,
    target: Option<&Prepared>,
    chords: &[Chord],
    request: &KeyRequest<'_>,
    call: &CallScope,
) -> Result<Done> {
    let Some(prepared) = target else { return Err(Error::invalid("key in background mode needs a target")) };
    if request.action != KeyAction::Press {
        return Err(Error::invalid("background mode only presses keys; key down and up need the real keyboard"));
    }
    message_keys::press(engine, prepared, chords, request.repeat, call)
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
