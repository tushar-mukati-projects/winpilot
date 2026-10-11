use deckhand_core::{Element, Result, TargetSpec, keys::NamedKey, protocol::TypeHow};
use deckhand_desktop::clipboard;

use super::{
    chords::named,
    clip::{NOTE_LIMIT, quoted},
    done::{Done, via_focus_click},
    focus::{Focused, keyboard_target},
    paste::paste,
    settle,
};
use crate::{call_scope::CallScope, engine::Engine};

const PASTE_THRESHOLD: usize = 200;
const KEPT_CLIPBOARD: &str = "the clipboard holds more than text, so the text was typed to keep it intact";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Typing {
    Keys,
    Paste,
    KeysToKeepClipboard,
}

pub struct TypeRequest<'a> {
    pub target: Option<&'a TargetSpec>,
    pub text: &'a str,
    pub enter: bool,
    pub how: TypeHow,
}

pub fn step(engine: &Engine, request: &TypeRequest<'_>, call: &CallScope) -> Result<Done> {
    let focused = keyboard_target(engine, request.target, call)?;
    let typing = choose_typing(request.how, request.text.chars().count(), clipboard_holds_only_text);
    let (method, restore_note) = enter_text(engine, &focused, request.text, typing, call)?;
    if request.enter {
        deckhand_input::press_chord(&named(NamedKey::Enter))?;
    }
    settle(engine, &focused.window, call);
    let kept = (typing == Typing::KeysToKeepClipboard).then(|| KEPT_CLIPBOARD.to_string());
    Ok(Done::by(via_focus_click(method, focused.clicked))
        .with_note(kept)
        .with_note(restore_note)
        .with_note(value_note(engine, focused.element.as_ref())))
}

pub fn choose_typing(how: TypeHow, chars: usize, clipboard_text_only: impl FnOnce() -> bool) -> Typing {
    let wants_paste = match how {
        TypeHow::Keys => false,
        TypeHow::Paste => true,
        TypeHow::Auto => chars > PASTE_THRESHOLD,
    };
    match (wants_paste && clipboard_text_only(), how) {
        (true, _) => Typing::Paste,
        (false, TypeHow::Paste) => Typing::KeysToKeepClipboard,
        (false, _) => Typing::Keys,
    }
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
        Typing::Paste => {
            paste(engine, &focused.window, focused.element.as_ref(), text, call).map(|note| ("paste", note))
        }
        Typing::Keys | Typing::KeysToKeepClipboard => deckhand_input::type_text(text).map(|()| ("keys", None)),
    }
}

pub fn value_note(engine: &Engine, element: Option<&Element>) -> Option<String> {
    let element = element.filter(|element| element.patterns.value && !element.state.password)?;
    let id = element.id;
    let value = engine.uia.run(move |uia| uia.value(id)).ok().flatten()?;
    Some(format!("value now {}", quoted(&value, NOTE_LIMIT)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn never_asked() -> bool {
        panic!("the clipboard should not be inspected")
    }

    #[test]
    fn short_text_is_typed_without_looking_at_the_clipboard() {
        assert_eq!(choose_typing(TypeHow::Auto, 200, never_asked), Typing::Keys);
        assert_eq!(choose_typing(TypeHow::Keys, 5000, never_asked), Typing::Keys);
    }

    #[test]
    fn long_text_is_pasted_only_over_a_text_clipboard() {
        assert_eq!(choose_typing(TypeHow::Auto, 201, || true), Typing::Paste);
        assert_eq!(choose_typing(TypeHow::Auto, 201, || false), Typing::Keys);
    }

    #[test]
    fn explicit_paste_keeps_a_rich_clipboard_by_typing() {
        assert_eq!(choose_typing(TypeHow::Paste, 3, || true), Typing::Paste);
        assert_eq!(choose_typing(TypeHow::Paste, 3, || false), Typing::KeysToKeepClipboard);
    }
}
