use winpilot_core::keys::{Chord, Key, Modifiers, NamedKey};

pub(crate) fn with_ctrl(key: char) -> Chord {
    Chord { modifiers: Modifiers { ctrl: true, ..Modifiers::NONE }, key: Some(Key::Char(key)) }
}

pub(crate) fn named(key: NamedKey) -> Chord {
    Chord { modifiers: Modifiers::NONE, key: Some(Key::Named(key)) }
}

#[cfg(test)]
mod tests {
    use winpilot_core::keys::parse_chord;

    use super::*;

    #[test]
    fn built_chords_match_their_parsed_forms() {
        assert_eq!(with_ctrl('v'), parse_chord("ctrl+v").unwrap());
        assert_eq!(with_ctrl('a'), parse_chord("ctrl+a").unwrap());
        assert_eq!(named(NamedKey::Enter), parse_chord("enter").unwrap());
        assert_eq!(named(NamedKey::Delete), parse_chord("delete").unwrap());
    }
}
