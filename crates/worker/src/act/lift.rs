use deckhand_core::{Result, keys::Chord};

use crate::call_scope::CallScope;

pub fn with_held_modifiers_lifted<T>(call: &CallScope, press: impl FnOnce() -> Result<T>) -> Result<T> {
    let lifted = held_modifiers(&call.held_keys());
    lifted.iter().rev().try_for_each(deckhand_input::release_chord)?;
    let pressed = press();
    let pressed_again = lifted.iter().try_for_each(deckhand_input::hold_chord);
    let value = pressed?;
    pressed_again.map(|()| value)
}

pub fn held_modifiers(held: &[Chord]) -> Vec<Chord> {
    held.iter()
        .filter(|chord| !chord.modifiers.is_empty())
        .map(|chord| Chord { modifiers: chord.modifiers, key: None })
        .collect()
}

#[cfg(test)]
mod tests {
    use deckhand_core::keys::parse_chord;

    use super::*;

    #[test]
    fn only_the_modifiers_of_held_chords_are_lifted() {
        let held = [parse_chord("shift").unwrap(), parse_chord("a").unwrap(), parse_chord("ctrl+b").unwrap()];
        assert_eq!(held_modifiers(&held), vec![parse_chord("shift").unwrap(), parse_chord("ctrl").unwrap()]);
        assert!(held_modifiers(&[]).is_empty());
    }

    #[test]
    fn nothing_held_means_the_press_runs_untouched() {
        let call = CallScope::relaxed();
        assert_eq!(with_held_modifiers_lifted(&call, || Ok(7)).unwrap(), 7);
        assert!(with_held_modifiers_lifted(&call, || Err::<(), _>(deckhand_core::Error::internal("x"))).is_err());
    }
}
