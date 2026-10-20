use winpilot_core::{Element, Error, Result, keys::NamedKey, protocol::TypeHow};
use winpilot_desktop::clipboard;
use winpilot_uia::Uia;

use super::{
    append,
    chords::named,
    clip::{NOTE_LIMIT, quoted},
    done::{Done, via_focus_click},
    field::is_writable,
    focus::{Focused, keyboard_target},
    landing::ensure_keys_land,
    lift::with_held_modifiers_lifted,
    message_keys,
    mode::Mode,
    pacing::{focused_field, wait_for_edit},
    paste::{Readback, paste, readback},
    settle,
    target::Prepared,
    text_focus::{has_printable, refusal},
};
use crate::{call_scope::CallScope, engine::Engine, overlay};

const PASTE_THRESHOLD: usize = 200;
const KEPT_CLIPBOARD: &str = "the clipboard holds more than text, so the text was typed to keep it intact";
const UNCONFIRMABLE: &str = "the field does not expose its value to confirm a paste, so the text was typed";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BackgroundTyping {
    Value,
    Message,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Typing {
    Keys,
    Paste(Readback),
    KeysToKeepClipboard,
    KeysToConfirm,
}

pub(crate) struct TypeRequest<'a> {
    pub(crate) text: &'a str,
    pub(crate) enter: bool,
    pub(crate) how: TypeHow,
}

impl Typing {
    fn note(&self) -> Option<String> {
        match self {
            Self::KeysToKeepClipboard => Some(KEPT_CLIPBOARD.to_string()),
            Self::KeysToConfirm => Some(UNCONFIRMABLE.to_string()),
            Self::Keys | Self::Paste(_) => None,
        }
    }
}

pub(crate) fn step(
    engine: &Engine,
    target: Option<&Prepared>,
    request: &TypeRequest<'_>,
    mode: Mode,
    call: &CallScope,
) -> Result<Done> {
    match (mode, target) {
        (Mode::Foreground, _) => type_for_real(engine, target, request, call),
        (Mode::Background, Some(prepared)) => type_in_background(engine, prepared, request, call),
        (Mode::Background, None) => Err(Error::invalid("type in background mode needs a target")),
    }
}

fn type_in_background(
    engine: &Engine,
    prepared: &Prepared,
    request: &TypeRequest<'_>,
    call: &CallScope,
) -> Result<Done> {
    match background_typing(request.how, request.enter, prepared.tracked_element()) {
        BackgroundTyping::Value => append::step(engine, prepared, request.text, call),
        BackgroundTyping::Message => message_keys::type_text(engine, prepared, request.text, request.enter, call),
    }
}

fn background_typing(how: TypeHow, enter: bool, tracked: Option<&Element>) -> BackgroundTyping {
    let writable = tracked.is_some_and(is_writable);
    if how == TypeHow::Auto && !enter && writable { BackgroundTyping::Value } else { BackgroundTyping::Message }
}

fn type_for_real(
    engine: &Engine,
    target: Option<&Prepared>,
    request: &TypeRequest<'_>,
    call: &CallScope,
) -> Result<Done> {
    let focused = keyboard_target(engine, target, call)?;
    if has_printable(request.text) {
        refuse_non_text_focus(engine)?;
    }
    if let Some(element) = &focused.element {
        overlay::focus_field(engine, call, element.rect);
    }
    let chars = request.text.chars().count();
    overlay::typing(engine, call, &focused.window);
    let typing =
        choose_typing(request.how, chars, || readback(engine, focused.element.as_ref()), clipboard_holds_only_text);
    let instead = typing.note();
    let (method, restore_note) = enter_text(engine, &focused, request.text, typing, call)?;
    if request.enter {
        press_enter(engine, &focused, call)?;
    }
    settle(engine, &focused.window, call);
    Ok(Done::by(via_focus_click(method, focused.clicked))
        .with_note(instead)
        .with_note(restore_note)
        .with_note(value_note(engine, focused.element.as_ref())))
}

fn choose_typing(
    how: TypeHow,
    chars: usize,
    readback: impl FnOnce() -> Option<Readback>,
    clipboard_text_only: impl FnOnce() -> bool,
) -> Typing {
    let wants_paste = match how {
        TypeHow::Keys => false,
        TypeHow::Paste => true,
        TypeHow::Auto => chars > PASTE_THRESHOLD,
    };
    if !wants_paste {
        return Typing::Keys;
    }
    let instead = |reason: Typing| if how == TypeHow::Paste { reason } else { Typing::Keys };
    let Some(readback) = readback() else { return instead(Typing::KeysToConfirm) };
    if !clipboard_text_only() {
        return instead(Typing::KeysToKeepClipboard);
    }
    Typing::Paste(readback)
}

fn refuse_non_text_focus(engine: &Engine) -> Result<()> {
    let focused = engine.uia.run(Uia::focused).ok().flatten();
    focused.as_ref().and_then(refusal).map_or(Ok(()), Err)
}

fn clipboard_holds_only_text() -> bool {
    clipboard::holds_only_text().unwrap_or(false)
}

fn enter_text(
    engine: &Engine,
    focused: &Focused,
    text: &str,
    typing: Typing,
    call: &CallScope,
) -> Result<(&'static str, Option<String>)> {
    match typing {
        Typing::Paste(readback) => paste(engine, &focused.window, &readback, text, call).map(|note| ("paste", note)),
        Typing::Keys | Typing::KeysToKeepClipboard | Typing::KeysToConfirm => {
            type_keys(engine, focused, text, call).map(|()| ("keys", None))
        }
    }
}

fn type_keys(engine: &Engine, focused: &Focused, text: &str, call: &CallScope) -> Result<()> {
    let field = focused_field(engine, focused);
    winpilot_input::type_text(text)?;
    wait_for_edit(engine, &focused.window, call, field.as_ref(), Some(text))
}

fn press_enter(engine: &Engine, focused: &Focused, call: &CallScope) -> Result<()> {
    engine.guard.check_running(call)?;
    ensure_keys_land(engine, focused)?;
    with_held_modifiers_lifted(call, || winpilot_input::press_chord(&named(NamedKey::Enter)))
}

pub(crate) fn value_note(engine: &Engine, element: Option<&Element>) -> Option<String> {
    readback(engine, element).map(|field| format!("value now {}", quoted(&field.before, NOTE_LIMIT)))
}

#[cfg(test)]
mod tests {
    use winpilot_core::ElementRef;

    use super::*;

    fn never_asked() -> bool {
        panic!("the clipboard should not be inspected")
    }

    fn never_read() -> Option<Readback> {
        panic!("the field should not be read")
    }

    fn field() -> Readback {
        Readback { id: ElementRef(4), before: String::new() }
    }

    #[test]
    fn background_typing_sets_writable_fields_and_messages_everything_else() {
        use winpilot_core::Role;

        use crate::test_fixture::element;

        let mut field = element(1, 1, Role::Edit, "Name");
        field.patterns.value = true;
        let canvas = element(2, 1, Role::Pane, "Editor");
        assert_eq!(background_typing(TypeHow::Auto, false, Some(&field)), BackgroundTyping::Value);
        assert_eq!(background_typing(TypeHow::Auto, true, Some(&field)), BackgroundTyping::Message);
        assert_eq!(background_typing(TypeHow::Keys, false, Some(&field)), BackgroundTyping::Message);
        assert_eq!(background_typing(TypeHow::Auto, false, Some(&canvas)), BackgroundTyping::Message);
        assert_eq!(background_typing(TypeHow::Auto, false, None), BackgroundTyping::Message);
        field.state.read_only = true;
        assert_eq!(background_typing(TypeHow::Auto, false, Some(&field)), BackgroundTyping::Message);
    }

    #[test]
    fn short_text_is_typed_without_looking_at_the_field_or_the_clipboard() {
        assert_eq!(choose_typing(TypeHow::Auto, 200, never_read, never_asked), Typing::Keys);
        assert_eq!(choose_typing(TypeHow::Keys, 5000, never_read, never_asked), Typing::Keys);
    }

    #[test]
    fn long_text_is_pasted_only_into_a_readable_field_over_a_text_clipboard() {
        assert_eq!(choose_typing(TypeHow::Auto, 201, || Some(field()), || true), Typing::Paste(field()));
        assert_eq!(choose_typing(TypeHow::Auto, 201, || Some(field()), || false), Typing::Keys);
        assert_eq!(choose_typing(TypeHow::Auto, 201, || None, never_asked), Typing::Keys);
    }

    #[test]
    fn explicit_paste_says_why_it_typed_instead() {
        assert_eq!(choose_typing(TypeHow::Paste, 3, || Some(field()), || true), Typing::Paste(field()));
        assert_eq!(choose_typing(TypeHow::Paste, 3, || Some(field()), || false), Typing::KeysToKeepClipboard);
        assert_eq!(choose_typing(TypeHow::Paste, 3, || None, never_asked), Typing::KeysToConfirm);
        assert_eq!(Typing::KeysToConfirm.note().as_deref(), Some(UNCONFIRMABLE));
        assert_eq!(Typing::KeysToKeepClipboard.note().as_deref(), Some(KEPT_CLIPBOARD));
        assert_eq!(Typing::Keys.note(), None);
    }
}
