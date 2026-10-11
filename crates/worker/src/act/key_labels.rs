use deckhand_core::keys::{Chord, Key, NamedKey};

const MAX_CHORDS: usize = 6;

pub fn chord_labels(chords: &[Chord]) -> Vec<Vec<String>> {
    chords.iter().take(MAX_CHORDS).map(chord_label).collect()
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
        chord_labels(&parse_keys(spec).unwrap())
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
        assert_eq!(labels("a b c d e f g h").len(), MAX_CHORDS);
        assert_eq!(labels("down"), vec![vec!["\u{2193}".to_string()]]);
        assert_eq!(labels("pgdn"), vec![vec!["PgDn".to_string()]]);
    }
}
