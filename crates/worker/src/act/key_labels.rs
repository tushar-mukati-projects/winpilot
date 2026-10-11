use deckhand_core::keys::{Chord, Key, NamedKey};

use crate::overlay::KeyChips;

const MAX_CHORDS: usize = 6;

pub fn key_chips(chords: &[Chord]) -> KeyChips {
    if chords.iter().all(types_text) {
        return KeyChips::Typing;
    }
    KeyChips::Keys(chords.iter().filter(|chord| !types_text(chord)).take(MAX_CHORDS).map(chord_label).collect())
}

fn types_text(chord: &Chord) -> bool {
    let modifiers = chord.modifiers;
    let only_shift = !(modifiers.ctrl || modifiers.alt || modifiers.win);
    only_shift && matches!(chord.key, Some(Key::Char(_) | Key::Named(NamedKey::Space)))
}

pub fn chord_text(chord: &Chord) -> String {
    chord_label(chord).join("+")
}

fn chord_label(chord: &Chord) -> Vec<String> {
    let modifiers = chord.modifiers;
    let held = [(modifiers.ctrl, "Ctrl"), (modifiers.alt, "Alt"), (modifiers.shift, "Shift"), (modifiers.win, "Win")];
    let mut labels: Vec<String> = held.iter().filter(|(down, _)| *down).map(|(_, name)| (*name).to_string()).collect();
    labels.extend(chord.key.map(key_label));
    labels
}

fn key_label(key: Key) -> String {
    match key {
        Key::Char(' ') => "Space".to_string(),
        Key::Char(letter) => letter.to_uppercase().collect(),
        Key::Function(number) => format!("F{number}"),
        Key::Named(named) => named_label(named).to_string(),
    }
}

const fn named_label(key: NamedKey) -> &'static str {
    match key {
        NamedKey::Enter => "Enter",
        NamedKey::Escape => "Esc",
        NamedKey::Tab => "Tab",
        NamedKey::Space => "Space",
        NamedKey::Backspace => "Backspace",
        NamedKey::Delete => "Del",
        NamedKey::Insert => "Ins",
        NamedKey::Home => "Home",
        NamedKey::End => "End",
        NamedKey::PageUp => "PgUp",
        NamedKey::PageDown => "PgDn",
        NamedKey::Up => "\u{2191}",
        NamedKey::Down => "\u{2193}",
        NamedKey::Left => "\u{2190}",
        NamedKey::Right => "\u{2192}",
        NamedKey::PrintScreen => "PrtSc",
        NamedKey::Pause => "Pause",
        NamedKey::CapsLock => "Caps",
        NamedKey::NumLock => "Num",
        NamedKey::ScrollLock => "ScrLk",
        NamedKey::ContextMenu => "Menu",
        NamedKey::VolumeUp => "Vol+",
        NamedKey::VolumeDown => "Vol\u{2212}",
        NamedKey::VolumeMute => "Mute",
        NamedKey::MediaPlayPause => "Play",
        NamedKey::MediaNext => "Next",
        NamedKey::MediaPrevious => "Prev",
        NamedKey::BrowserBack => "Back",
        NamedKey::BrowserForward => "Forward",
    }
}

#[cfg(test)]
mod tests {
    use deckhand_core::keys::parse_keys;

    use super::*;

    fn labels(spec: &str) -> Vec<Vec<String>> {
        match key_chips(&parse_keys(spec).unwrap()) {
            KeyChips::Keys(labels) => labels,
            KeyChips::Typing => panic!("{spec} should show keycaps"),
        }
    }

    fn chips(spec: &str) -> KeyChips {
        key_chips(&parse_keys(spec).unwrap())
    }

    #[test]
    fn chords_read_as_one_word() {
        assert_eq!(chord_text(&deckhand_core::keys::parse_chord("ctrl+shift+a").unwrap()), "Ctrl+Shift+A");
        assert_eq!(chord_text(&deckhand_core::keys::parse_chord("shift").unwrap()), "Shift");
    }

    #[test]
    fn chords_show_modifiers_then_the_key() {
        assert_eq!(labels("ctrl+s"), vec![vec!["Ctrl".to_string(), "S".to_string()]]);
        assert_eq!(labels("ctrl+shift+tab"), vec![vec!["Ctrl".to_string(), "Shift".to_string(), "Tab".to_string()]]);
        assert_eq!(labels("alt+f4"), vec![vec!["Alt".to_string(), "F4".to_string()]]);
    }

    #[test]
    fn sequences_become_separate_chords_and_are_capped() {
        assert_eq!(labels("ctrl+k ctrl+s").len(), 2);
        assert_eq!(labels("f1 f2 f3 f4 f5 f6 f7 f8").len(), MAX_CHORDS);
        assert_eq!(labels("down"), vec![vec!["\u{2193}".to_string()]]);
        assert_eq!(labels("pgdn"), vec![vec!["PgDn".to_string()]]);
    }

    #[test]
    fn printable_keys_show_only_as_typing() {
        assert_eq!(chips("a"), KeyChips::Typing);
        assert_eq!(chips("shift+a"), KeyChips::Typing);
        assert_eq!(chips("space"), KeyChips::Typing);
        assert_eq!(chips("h e l l o"), KeyChips::Typing);
        assert_eq!(chips("1 plus 2"), KeyChips::Typing);
    }

    #[test]
    fn named_keys_and_modifier_chords_keep_their_keycaps_without_the_typed_characters() {
        assert_eq!(chips("shift+tab"), KeyChips::Keys(vec![vec!["Shift".to_string(), "Tab".to_string()]]));
        assert_eq!(chips("shift"), KeyChips::Keys(vec![vec!["Shift".to_string()]]));
        assert_eq!(chips("ctrl+space"), KeyChips::Keys(vec![vec!["Ctrl".to_string(), "Space".to_string()]]));
        assert_eq!(
            chips("ctrl+a x y enter"),
            KeyChips::Keys(vec![vec!["Ctrl".to_string(), "A".to_string()], vec!["Enter".to_string()]])
        );
    }
}
