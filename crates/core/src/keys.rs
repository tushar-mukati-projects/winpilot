mod names;
mod tokens;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

use names::{char_alias, function_number, modifier_named, named_key, single_char};
use tokens::{group_chords, tokenize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Modifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
}

impl Modifiers {
    pub const NONE: Self = Self { ctrl: false, alt: false, shift: false, win: false };

    pub const fn is_empty(self) -> bool {
        !(self.ctrl || self.alt || self.shift || self.win)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NamedKey {
    Enter,
    Escape,
    Tab,
    Space,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Up,
    Down,
    Left,
    Right,
    PrintScreen,
    Pause,
    CapsLock,
    NumLock,
    ScrollLock,
    ContextMenu,
    VolumeUp,
    VolumeDown,
    VolumeMute,
    MediaPlayPause,
    MediaNext,
    MediaPrevious,
    BrowserBack,
    BrowserForward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    Char(char),
    Named(NamedKey),
    Function(u8),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Chord {
    pub modifiers: Modifiers,
    pub key: Option<Key>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Win,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Part {
    Modifier(Modifier),
    Key(Key),
}

const FUNCTION_KEYS: std::ops::RangeInclusive<u8> = 1..=24;

pub fn parse_keys(spec: &str) -> Result<Vec<Chord>> {
    let tokens = tokenize(spec);
    let chords = group_chords(&tokens, spec)?;
    if chords.is_empty() {
        return Err(empty_spec(spec));
    }
    chords.iter().map(|parts| build_chord(parts, spec)).collect()
}

pub fn parse_chord(spec: &str) -> Result<Chord> {
    match parse_keys(spec)?.as_slice() {
        [chord] => Ok(*chord),
        chords => Err(Error::invalid(format!("expected one key chord in '{spec}', found {}", chords.len()))),
    }
}

fn empty_spec(spec: &str) -> Error {
    if spec.trim().is_empty() {
        Error::invalid("empty key spec")
    } else {
        Error::invalid(format!("no keys in '{spec}'"))
    }
}

fn build_chord(parts: &[&str], spec: &str) -> Result<Chord> {
    let mut modifiers = Modifiers::NONE;
    let mut key: Option<(&str, Key)> = None;
    for &part in parts {
        match classify(part, spec)? {
            Part::Modifier(modifier) => modifiers = with_modifier(modifiers, modifier),
            Part::Key(found) => {
                if let Some((previous, _)) = key {
                    return Err(two_keys(previous, part, spec));
                }
                key = Some((part, found));
            }
        }
    }
    Ok(Chord { modifiers, key: key.map(|(_, found)| found) })
}

fn two_keys(first: &str, second: &str, spec: &str) -> Error {
    Error::invalid(format!(
        "two keys '{first}' and '{second}' in one chord in '{spec}'; a chord is modifiers plus at most one key"
    ))
}

const fn with_modifier(modifiers: Modifiers, modifier: Modifier) -> Modifiers {
    match modifier {
        Modifier::Ctrl => Modifiers { ctrl: true, ..modifiers },
        Modifier::Alt => Modifiers { alt: true, ..modifiers },
        Modifier::Shift => Modifiers { shift: true, ..modifiers },
        Modifier::Win => Modifiers { win: true, ..modifiers },
    }
}

fn classify(part: &str, spec: &str) -> Result<Part> {
    let lower = part.to_ascii_lowercase();
    if let Some(modifier) = modifier_named(&lower) {
        return Ok(Part::Modifier(modifier));
    }
    if let Some(named) = named_key(&lower) {
        return Ok(Part::Key(Key::Named(named)));
    }
    if let Some(number) = function_number(&lower) {
        return function_key(number, part, spec).map(Part::Key);
    }
    char_alias(&lower)
        .or_else(|| single_char(part))
        .map(|ch| Part::Key(Key::Char(ch)))
        .ok_or_else(|| Error::invalid(format!("unknown key '{part}' in '{spec}'")))
}

fn function_key(number: u32, part: &str, spec: &str) -> Result<Key> {
    u8::try_from(number)
        .ok()
        .filter(|number| FUNCTION_KEYS.contains(number))
        .map(Key::Function)
        .ok_or_else(|| Error::invalid(format!("function key '{part}' is out of range in '{spec}'; use f1 to f24")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorCode;

    const CTRL: Modifiers = Modifiers { ctrl: true, ..Modifiers::NONE };
    const ALT: Modifiers = Modifiers { alt: true, ..Modifiers::NONE };
    const SHIFT: Modifiers = Modifiers { shift: true, ..Modifiers::NONE };
    const WIN: Modifiers = Modifiers { win: true, ..Modifiers::NONE };

    fn chord(modifiers: Modifiers, key: Key) -> Chord {
        Chord { modifiers, key: Some(key) }
    }

    fn modifiers_only(modifiers: Modifiers) -> Chord {
        Chord { modifiers, key: None }
    }

    fn ch(c: char) -> Key {
        Key::Char(c)
    }

    fn named(key: NamedKey) -> Key {
        Key::Named(key)
    }

    fn named_chord(key: NamedKey) -> Chord {
        chord(Modifiers::NONE, named(key))
    }

    fn error_message(spec: &str) -> String {
        let error = parse_keys(spec).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument, "{spec}");
        error.message
    }

    #[test]
    fn parses_a_simple_chord() {
        assert_eq!(parse_keys("ctrl+s").unwrap(), [chord(CTRL, ch('s'))]);
    }

    #[test]
    fn matching_is_case_insensitive() {
        let expected = chord(Modifiers { ctrl: true, shift: true, ..Modifiers::NONE }, ch('s'));
        assert_eq!(parse_keys("CTRL+Shift+S").unwrap(), [expected]);
        assert_eq!(parse_keys("PgUp").unwrap(), [chord(Modifiers::NONE, named(NamedKey::PageUp))]);
    }

    #[test]
    fn spaces_around_plus_are_allowed() {
        let expected = chord(Modifiers { ctrl: true, shift: true, ..Modifiers::NONE }, ch('s'));
        assert_eq!(parse_keys("ctrl + shift + s").unwrap(), [expected]);
        assert_eq!(parse_keys("ctrl+ shift +s").unwrap(), [expected]);
        assert_eq!(parse_keys("  ctrl\t+\ts  ").unwrap(), [chord(CTRL, ch('s'))]);
    }

    #[test]
    fn sequences_split_on_whitespace_and_commas() {
        let expected = [chord(CTRL, ch('k')), chord(CTRL, ch('s'))];
        assert_eq!(parse_keys("ctrl+k ctrl+s").unwrap(), expected);
        assert_eq!(parse_keys("ctrl+k,ctrl+s").unwrap(), expected);
        assert_eq!(parse_keys("ctrl+k , ctrl+s").unwrap(), expected);
        assert_eq!(parse_keys(" ,ctrl+k,, ctrl+s, ").unwrap(), expected);
        assert_eq!(parse_keys("ctrl + k   ctrl + s").unwrap(), expected);
    }

    #[test]
    fn single_keys_form_their_own_chords() {
        let keys = parse_keys("a b enter").unwrap();
        assert_eq!(
            keys,
            [chord(Modifiers::NONE, ch('a')), chord(Modifiers::NONE, ch('b')), named_chord(NamedKey::Enter)]
        );
    }

    #[test]
    fn every_modifier_alias() {
        let cases = [
            ("ctrl", CTRL),
            ("control", CTRL),
            ("ctl", CTRL),
            ("alt", ALT),
            ("option", ALT),
            ("shift", SHIFT),
            ("win", WIN),
            ("windows", WIN),
            ("meta", WIN),
            ("super", WIN),
        ];
        for (alias, modifiers) in cases {
            assert_eq!(parse_chord(&format!("{alias}+x")).unwrap(), chord(modifiers, ch('x')), "{alias}");
            assert_eq!(parse_chord(&alias.to_uppercase()).unwrap(), modifiers_only(modifiers), "{alias}");
        }
    }

    #[test]
    fn every_named_key_alias() {
        let cases = [
            ("enter", NamedKey::Enter),
            ("return", NamedKey::Enter),
            ("esc", NamedKey::Escape),
            ("escape", NamedKey::Escape),
            ("tab", NamedKey::Tab),
            ("space", NamedKey::Space),
            ("backspace", NamedKey::Backspace),
            ("bksp", NamedKey::Backspace),
            ("back", NamedKey::Backspace),
            ("delete", NamedKey::Delete),
            ("del", NamedKey::Delete),
            ("insert", NamedKey::Insert),
            ("ins", NamedKey::Insert),
            ("home", NamedKey::Home),
            ("end", NamedKey::End),
            ("pageup", NamedKey::PageUp),
            ("pgup", NamedKey::PageUp),
            ("pagedown", NamedKey::PageDown),
            ("pgdn", NamedKey::PageDown),
            ("up", NamedKey::Up),
            ("arrowup", NamedKey::Up),
            ("down", NamedKey::Down),
            ("arrowdown", NamedKey::Down),
            ("left", NamedKey::Left),
            ("arrowleft", NamedKey::Left),
            ("right", NamedKey::Right),
            ("arrowright", NamedKey::Right),
            ("printscreen", NamedKey::PrintScreen),
            ("prtsc", NamedKey::PrintScreen),
            ("prtscr", NamedKey::PrintScreen),
            ("pause", NamedKey::Pause),
            ("break", NamedKey::Pause),
            ("capslock", NamedKey::CapsLock),
            ("numlock", NamedKey::NumLock),
            ("scrolllock", NamedKey::ScrollLock),
            ("contextmenu", NamedKey::ContextMenu),
            ("menu", NamedKey::ContextMenu),
            ("apps", NamedKey::ContextMenu),
            ("application", NamedKey::ContextMenu),
            ("volumeup", NamedKey::VolumeUp),
            ("volumedown", NamedKey::VolumeDown),
            ("volumemute", NamedKey::VolumeMute),
            ("mute", NamedKey::VolumeMute),
            ("playpause", NamedKey::MediaPlayPause),
            ("mediaplaypause", NamedKey::MediaPlayPause),
            ("nexttrack", NamedKey::MediaNext),
            ("medianext", NamedKey::MediaNext),
            ("prevtrack", NamedKey::MediaPrevious),
            ("mediaprevious", NamedKey::MediaPrevious),
            ("browserback", NamedKey::BrowserBack),
            ("browserforward", NamedKey::BrowserForward),
        ];
        for (alias, key) in cases {
            assert_eq!(parse_chord(alias).unwrap(), named_chord(key), "{alias}");
            assert_eq!(parse_chord(&alias.to_uppercase()).unwrap(), named_chord(key), "{alias}");
            assert_eq!(parse_chord(&format!("alt+{alias}")).unwrap(), chord(ALT, named(key)), "{alias}");
        }
    }

    #[test]
    fn risky_aliases_are_not_modifiers() {
        assert_eq!(parse_chord("menu").unwrap(), named_chord(NamedKey::ContextMenu));
        assert_eq!(parse_chord("shift+menu").unwrap(), chord(SHIFT, named(NamedKey::ContextMenu)));
        assert_eq!(error_message("cmd+l"), "unknown key 'cmd' in 'cmd+l'");
        assert_eq!(
            error_message("menu+f"),
            "two keys 'menu' and 'f' in one chord in 'menu+f'; a chord is modifiers plus at most one key"
        );
    }

    #[test]
    fn function_keys_one_to_twenty_four() {
        for number in 1..=24u8 {
            let expected = chord(Modifiers::NONE, Key::Function(number));
            assert_eq!(parse_chord(&format!("f{number}")).unwrap(), expected);
            assert_eq!(parse_chord(&format!("F{number}")).unwrap(), expected);
        }
        assert_eq!(parse_chord("alt+f4").unwrap(), chord(ALT, Key::Function(4)));
    }

    #[test]
    fn function_keys_out_of_range_are_rejected() {
        for spec in ["f0", "f25", "F99", "ctrl+f300", "f99999999999"] {
            let message = error_message(spec);
            assert!(message.contains("out of range"), "{message}");
            assert!(message.contains("f1 to f24"), "{message}");
        }
        assert_eq!(error_message("f25"), "function key 'f25' is out of range in 'f25'; use f1 to f24");
    }

    #[test]
    fn lone_f_is_a_character() {
        assert_eq!(parse_chord("f").unwrap(), chord(Modifiers::NONE, ch('f')));
        assert_eq!(parse_chord("ctrl+F").unwrap(), chord(CTRL, ch('f')));
    }

    #[test]
    fn single_characters_are_char_keys() {
        assert_eq!(parse_chord("a").unwrap(), chord(Modifiers::NONE, ch('a')));
        assert_eq!(parse_chord("A").unwrap(), chord(Modifiers::NONE, ch('a')));
        assert_eq!(parse_chord("ctrl+1").unwrap(), chord(CTRL, ch('1')));
        assert_eq!(parse_chord("ctrl+/").unwrap(), chord(CTRL, ch('/')));
        assert_eq!(parse_chord("shift+=").unwrap(), chord(SHIFT, ch('=')));
        assert_eq!(parse_chord("ctrl+[").unwrap(), chord(CTRL, ch('[')));
        assert_eq!(parse_chord("'").unwrap(), chord(Modifiers::NONE, ch('\'')));
    }

    #[test]
    fn non_ascii_letters_are_lowercased() {
        assert_eq!(parse_chord("É").unwrap(), chord(Modifiers::NONE, ch('é')));
        assert_eq!(parse_chord("ctrl+ß").unwrap(), chord(CTRL, ch('ß')));
        assert_eq!(parse_chord("İ").unwrap(), chord(Modifiers::NONE, ch('İ')));
    }

    #[test]
    fn punctuation_aliases() {
        assert_eq!(parse_chord("ctrl+plus").unwrap(), chord(CTRL, ch('+')));
        assert_eq!(parse_chord("ctrl+minus").unwrap(), chord(CTRL, ch('-')));
        assert_eq!(parse_chord("ctrl+-").unwrap(), chord(CTRL, ch('-')));
        assert_eq!(parse_chord("ctrl+comma").unwrap(), chord(CTRL, ch(',')));
        assert_eq!(parse_chord("COMMA").unwrap(), chord(Modifiers::NONE, ch(',')));
        assert_eq!(parse_chord("space").unwrap(), named_chord(NamedKey::Space));
    }

    #[test]
    fn literal_plus_forms() {
        let expected = chord(CTRL, ch('+'));
        assert_eq!(parse_chord("ctrl++").unwrap(), expected);
        assert_eq!(parse_chord("ctrl+plus").unwrap(), expected);
        assert_eq!(parse_chord("ctrl + +").unwrap(), expected);
        assert_eq!(parse_chord("+").unwrap(), chord(Modifiers::NONE, ch('+')));
        assert_eq!(parse_keys("ctrl++ a").unwrap(), [expected, chord(Modifiers::NONE, ch('a'))]);
        assert_eq!(parse_keys("ctrl++,shift+plus").unwrap(), [expected, chord(SHIFT, ch('+'))]);
    }

    #[test]
    fn modifier_only_chords_have_no_key() {
        assert_eq!(parse_chord("win").unwrap(), modifiers_only(WIN));
        let both = Modifiers { ctrl: true, shift: true, ..Modifiers::NONE };
        assert_eq!(parse_chord("ctrl+shift").unwrap(), modifiers_only(both));
        assert!(parse_chord("ctrl+shift").unwrap().key.is_none());
    }

    #[test]
    fn repeated_modifiers_are_harmless() {
        assert_eq!(parse_chord("ctrl+control+s").unwrap(), chord(CTRL, ch('s')));
    }

    #[test]
    fn all_four_modifiers_together() {
        let all = Modifiers { ctrl: true, alt: true, shift: true, win: true };
        assert_eq!(parse_chord("ctrl+alt+shift+win+backspace").unwrap(), chord(all, named(NamedKey::Backspace)));
    }

    #[test]
    fn empty_specs_are_rejected() {
        assert_eq!(error_message(""), "empty key spec");
        assert_eq!(error_message("   "), "empty key spec");
        assert_eq!(error_message(" , ,"), "no keys in ' , ,'");
    }

    #[test]
    fn unknown_names_are_rejected() {
        assert_eq!(error_message("foo"), "unknown key 'foo' in 'foo'");
        assert_eq!(error_message("ctrl+Bogus"), "unknown key 'Bogus' in 'ctrl+Bogus'");
        assert_eq!(error_message("ctrl+k ctrl+zz"), "unknown key 'zz' in 'ctrl+k ctrl+zz'");
        assert_eq!(error_message("page-up"), "unknown key 'page-up' in 'page-up'");
    }

    #[test]
    fn empty_parts_are_rejected() {
        assert_eq!(error_message("ctrl+"), "empty key after '+' in 'ctrl+'");
        assert_eq!(error_message("ctrl+ "), "empty key after '+' in 'ctrl+ '");
        assert_eq!(error_message("ctrl+++"), "empty key after '+' in 'ctrl+++'");
        assert_eq!(error_message("ctrl+,a"), "empty key after '+' in 'ctrl+,a'; write a comma key as 'comma'");
        assert_eq!(error_message("+a"), "empty key before '+' in '+a'");
        assert_eq!(error_message("ctrl++a"), "empty key before '+' in 'ctrl++a'");
    }

    #[test]
    fn two_keys_in_one_chord_are_rejected() {
        assert_eq!(
            error_message("a+b"),
            "two keys 'a' and 'b' in one chord in 'a+b'; a chord is modifiers plus at most one key"
        );
        assert!(error_message("ctrl+enter+tab").contains("'enter' and 'tab'"));
        assert!(error_message("ctrl+f4+plus").contains("'f4' and 'plus'"));
    }

    #[test]
    fn parse_chord_needs_exactly_one_chord() {
        let error = parse_chord("ctrl+k ctrl+s").unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
        assert_eq!(error.message, "expected one key chord in 'ctrl+k ctrl+s', found 2");
        assert_eq!(parse_chord("").unwrap_err().message, "empty key spec");
        assert_eq!(parse_chord(" ctrl+s ").unwrap(), chord(CTRL, ch('s')));
    }

    #[test]
    fn default_kill_hotkey_parses() {
        let ctrl_alt = Modifiers { ctrl: true, alt: true, shift: false, win: false };
        assert_eq!(parse_chord("ctrl+alt+escape").unwrap(), chord(ctrl_alt, named(NamedKey::Escape)));
    }

    #[test]
    fn modifiers_is_empty() {
        assert!(Modifiers::NONE.is_empty());
        assert!(!CTRL.is_empty());
        assert!(!WIN.is_empty());
    }
}
