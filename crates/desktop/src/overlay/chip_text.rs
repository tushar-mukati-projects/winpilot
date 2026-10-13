use super::keycaps::Item;

const TYPING: &str = "typing\u{2026}";
const SPOKEN_TYPING: &str = "typing";
const SHIFT: &str = "Shift";
const MODIFIERS: [&str; 4] = ["Ctrl", "Alt", SHIFT, "Win"];
const SPACE: &str = "Space";
const ARROWS: [char; 4] = ['\u{2190}', '\u{2191}', '\u{2192}', '\u{2193}'];

#[derive(Debug, Clone, PartialEq, Eq)]
enum KeyChip {
    Chord(Vec<String>),
    Typing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChipText(Vec<KeyChip>);

impl ChipText {
    pub(crate) fn typing() -> Self {
        Self(vec![KeyChip::Typing])
    }

    pub(crate) fn keys(chords: Vec<Vec<String>>) -> Self {
        let mut chips: Vec<KeyChip> = Vec::new();
        for chord in chords.into_iter().filter(|chord| !chord.is_empty()) {
            let chip = if types_text(&chord) { KeyChip::Typing } else { KeyChip::Chord(chord) };
            if chip != KeyChip::Typing || chips.last() != Some(&KeyChip::Typing) {
                chips.push(chip);
            }
        }
        Self(chips)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn spoken(&self) -> String {
        let words: Vec<String> = self
            .0
            .iter()
            .map(|chip| match chip {
                KeyChip::Typing => SPOKEN_TYPING.to_string(),
                KeyChip::Chord(keys) => keys.join("+"),
            })
            .collect();
        words.join(", ")
    }

    pub(crate) fn items(&self) -> Vec<Item> {
        let mut items = Vec::new();
        for (index, chip) in self.0.iter().enumerate() {
            if index > 0 {
                items.push(Item::Gap);
            }
            match chip {
                KeyChip::Typing => items.push(Item::Cap(TYPING.to_string())),
                KeyChip::Chord(keys) => push_chord(&mut items, keys),
            }
        }
        items
    }
}

fn push_chord(items: &mut Vec<Item>, keys: &[String]) {
    for (index, key) in keys.iter().enumerate() {
        if index > 0 {
            items.push(Item::Plus);
        }
        items.push(Item::Cap(key.clone()));
    }
}

fn types_text(chord: &[String]) -> bool {
    let (modifiers, keys): (Vec<&String>, Vec<&String>) =
        chord.iter().partition(|key| MODIFIERS.contains(&key.as_str()));
    let shortcut = modifiers.iter().any(|modifier| modifier.as_str() != SHIFT);
    let [key] = keys.as_slice() else { return false };
    !shortcut && prints_a_character(key)
}

fn prints_a_character(label: &str) -> bool {
    let mut characters = label.chars();
    match (characters.next(), characters.next()) {
        (Some(only), None) => !ARROWS.contains(&only) && !only.is_control(),
        _ => label == SPACE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(keys: &[&str]) -> Vec<String> {
        keys.iter().map(|key| (*key).to_string()).collect()
    }

    fn caps(text: &ChipText) -> Vec<String> {
        text.items()
            .into_iter()
            .map(|item| match item {
                Item::Cap(label) => label,
                Item::Plus => "+".to_string(),
                Item::Gap => "|".to_string(),
            })
            .collect()
    }

    #[test]
    fn chords_become_caps_with_plus_signs_and_gaps() {
        let text = ChipText::keys(vec![chord(&["Ctrl", "S"]), chord(&["Enter"])]);
        assert_eq!(caps(&text), ["Ctrl", "+", "S", "|", "Enter"]);
    }

    #[test]
    fn typing_never_shows_the_text_or_its_length() {
        assert_eq!(caps(&ChipText::typing()), ["typing\u{2026}"]);
    }

    #[test]
    fn printable_keys_collapse_into_one_typing_chip() {
        let typed = ChipText::keys(vec![chord(&["P"]), chord(&["Shift", "4"]), chord(&["Space"]), chord(&["\u{e9}"])]);
        assert_eq!(caps(&typed), ["typing\u{2026}"]);
        let mixed = ChipText::keys(vec![chord(&["Ctrl", "A"]), chord(&["H"]), chord(&["I"]), chord(&["Enter"])]);
        assert_eq!(caps(&mixed), ["Ctrl", "+", "A", "|", "typing\u{2026}", "|", "Enter"]);
    }

    #[test]
    fn named_keys_arrows_and_shortcuts_stay_visible() {
        let text = ChipText::keys(vec![
            chord(&["\u{2193}"]),
            chord(&["Shift", "Tab"]),
            chord(&["Alt", "F4"]),
            chord(&["Ctrl", "Shift", "S"]),
            chord(&["Win"]),
        ]);
        assert_eq!(text.items().iter().filter(|item| **item == Item::Cap(TYPING.to_string())).count(), 0);
        assert_eq!(caps(&text)[0], "\u{2193}");
    }

    #[test]
    fn spoken_chips_join_keys_with_plus_and_chips_with_commas() {
        let mixed = ChipText::keys(vec![chord(&["Ctrl", "A"]), chord(&["H"]), chord(&["Enter"])]);
        assert_eq!(mixed.spoken(), "Ctrl+A, typing, Enter");
        assert_eq!(ChipText::typing().spoken(), "typing");
        assert_eq!(ChipText::keys(Vec::new()).spoken(), "");
    }

    #[test]
    fn empty_chords_show_nothing() {
        assert!(ChipText::keys(Vec::new()).is_empty());
        assert!(ChipText::keys(vec![Vec::new()]).is_empty());
        assert!(!ChipText::typing().is_empty());
    }
}
