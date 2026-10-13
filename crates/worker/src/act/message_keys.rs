use deckhand_core::{Element, Result, WindowInfo, WindowRef, keys::Chord, protocol::MouseButton};

use super::{
    by_pattern::by_pattern,
    done::{Done, via_focus_click},
    key_labels::key_chips,
    message::{MESSAGE, click_spot, require_not_in_use, spot},
    target::Prepared,
    typing::value_note,
};
use crate::{call_scope::CallScope, engine::Engine, overlay, resolve::Reach};

const ENTER: &str = "\n";

struct Keyboard {
    window: WindowInfo,
    receiver: WindowRef,
    element: Option<Element>,
    clicked: bool,
    note: Option<String>,
}

pub(crate) fn type_text(
    engine: &Engine,
    prepared: &Prepared,
    text: &str,
    enter: bool,
    call: &CallScope,
) -> Result<Done> {
    let keyboard = keyboard(engine, prepared, call)?;
    if keyboard.window.on_current_desktop {
        if let Some(element) = &keyboard.element {
            overlay::focus_field(engine, call, element.rect);
        }
        overlay::typing(engine, call, &keyboard.window);
    }
    let typed = if enter { format!("{text}{ENTER}") } else { text.to_string() };
    let check = || engine.guard.check_running(call);
    let ((), note) =
        by_pattern(engine, &keyboard.window, call, || deckhand_input::message_text(keyboard.receiver, &typed, check))?;
    Ok(Done::by(via_focus_click(MESSAGE, keyboard.clicked))
        .with_note(keyboard.note)
        .with_note(note)
        .with_note(value_note(engine, prepared.tracked_element())))
}

pub(crate) fn press(
    engine: &Engine,
    prepared: &Prepared,
    chords: &[Chord],
    repeat: u32,
    call: &CallScope,
) -> Result<Done> {
    let keyboard = keyboard(engine, prepared, call)?;
    if keyboard.window.on_current_desktop {
        overlay::key_chips(engine, call, &keyboard.window, key_chips(chords));
    }
    let check = || engine.guard.check_running(call);
    let ((), note) = by_pattern(engine, &keyboard.window, call, || {
        deckhand_input::message_keys(keyboard.receiver, chords, repeat, check)
    })?;
    Ok(Done::by(via_focus_click(MESSAGE, keyboard.clicked)).with_note(keyboard.note).with_note(note))
}

fn keyboard(engine: &Engine, prepared: &Prepared, call: &CallScope) -> Result<Keyboard> {
    if matches!(prepared.reach, Reach::Window) {
        return window_keyboard(engine, &prepared.window, call);
    }
    let spot = spot(engine, prepared, call)?;
    let note = click_spot(engine, &spot, MouseButton::Left, 1, call)?;
    let element = prepared.tracked_element().cloned();
    Ok(Keyboard { receiver: spot.target.window, window: spot.window, element, clicked: true, note })
}

fn window_keyboard(engine: &Engine, window: &WindowInfo, call: &CallScope) -> Result<Keyboard> {
    let window = deckhand_desktop::window(window.hwnd)?;
    engine.guard.before_patterns(Some(&window), call)?;
    require_not_in_use(&window)?;
    let receiver = deckhand_input::keyboard_window(window.hwnd)?;
    Ok(Keyboard { window, receiver, element: None, clicked: false, note: None })
}
