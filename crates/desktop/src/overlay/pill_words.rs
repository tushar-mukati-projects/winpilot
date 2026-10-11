use super::presence::Tone;

const STOPPED: &str = "Stopped";
const SEPARATOR: &str = " \u{00B7} ";
const CONTROLLING: &str = " is controlling";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Wording {
    Full,
    Short,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PillWords {
    pub(crate) lead: String,
    pub(crate) hint: Option<String>,
}

pub(crate) fn hotkey_label(spec: &str) -> Option<String> {
    let keys: Vec<String> = spec.split('+').map(str::trim).filter(|key| !key.is_empty()).map(capitalized).collect();
    (!keys.is_empty()).then(|| keys.join("+"))
}

pub(crate) fn pill_words(tone: Tone, wording: Wording, label: &str, hotkey: Option<&str>) -> PillWords {
    let hotkey = hotkey.filter(|_| wording == Wording::Full);
    match tone {
        Tone::Working => PillWords {
            lead: label.to_string(),
            hint: Some(match hotkey {
                Some(hotkey) => format!("{CONTROLLING}{SEPARATOR}{hotkey} to stop"),
                None => CONTROLLING.to_string(),
            }),
        },
        Tone::Stopped => PillWords {
            lead: STOPPED.to_string(),
            hint: hotkey.map(|hotkey| format!("{SEPARATOR}press {hotkey} to resume")),
        },
    }
}

pub(crate) fn recording_words(label: &str, seconds: u64) -> PillWords {
    PillWords { lead: label.to_string(), hint: Some(format!(" is recording{SEPARATOR}{}", elapsed_label(seconds))) }
}

pub(crate) fn elapsed_label(seconds: u64) -> String {
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if hours > 0 { format!("{hours}:{minutes:02}:{seconds:02}") } else { format!("{minutes:02}:{seconds:02}") }
}

fn capitalized(key: &str) -> String {
    let mut letters = key.chars();
    letters
        .next()
        .map(|first| first.to_uppercase().chain(letters.flat_map(char::to_lowercase)).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOTKEY: Option<&str> = Some("Ctrl+Alt+Shift+Backspace");

    fn joined(words: &PillWords) -> String {
        format!("{}{}", words.lead, words.hint.as_deref().unwrap_or_default())
    }

    #[test]
    fn hotkeys_are_title_cased() {
        assert_eq!(hotkey_label("ctrl+alt+shift+backspace").as_deref(), Some("Ctrl+Alt+Shift+Backspace"));
        assert_eq!(hotkey_label(" CTRL + f12 ").as_deref(), Some("Ctrl+F12"));
        assert_eq!(hotkey_label(""), None);
        assert_eq!(hotkey_label("+"), None);
    }

    #[test]
    fn the_full_label_names_the_agent_and_the_stop_hotkey() {
        let words = pill_words(Tone::Working, Wording::Full, "Codex", HOTKEY);
        assert_eq!(words.lead, "Codex");
        assert_eq!(joined(&words), "Codex is controlling \u{00B7} Ctrl+Alt+Shift+Backspace to stop");
    }

    #[test]
    fn the_stopped_label_names_the_resume_hotkey() {
        let words = pill_words(Tone::Stopped, Wording::Full, "Codex", HOTKEY);
        assert_eq!(joined(&words), "Stopped \u{00B7} press Ctrl+Alt+Shift+Backspace to resume");
    }

    #[test]
    fn short_labels_and_missing_hotkeys_drop_the_hint() {
        assert_eq!(joined(&pill_words(Tone::Working, Wording::Short, "Claude", HOTKEY)), "Claude is controlling");
        assert_eq!(joined(&pill_words(Tone::Working, Wording::Full, "Claude", None)), "Claude is controlling");
        assert_eq!(
            pill_words(Tone::Stopped, Wording::Full, "Claude", None),
            PillWords { lead: STOPPED.to_string(), hint: None }
        );
    }

    #[test]
    fn recording_shows_the_agent_and_the_elapsed_time() {
        assert_eq!(elapsed_label(12), "00:12");
        assert_eq!(elapsed_label(754), "12:34");
        assert_eq!(elapsed_label(3723), "1:02:03");
        assert_eq!(joined(&recording_words("Gemini", 12)), "Gemini is recording \u{00B7} 00:12");
    }
}
