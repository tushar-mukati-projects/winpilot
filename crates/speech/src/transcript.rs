#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    pub start_ms: u64,
    pub end_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transcript {
    pub language: String,
    pub segments: Vec<Segment>,
}

pub fn clock(milliseconds: u64) -> String {
    let seconds = milliseconds / 1_000;
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

pub(crate) fn cleaned_text(raw: &str) -> Option<String> {
    let text = raw.trim();
    if text.is_empty() || is_blank_marker(text) {
        return None;
    }
    Some(text.to_string())
}

fn is_blank_marker(text: &str) -> bool {
    let inner: String = text
        .trim_matches(|character| matches!(character, '[' | ']' | '(' | ')' | ' '))
        .to_ascii_lowercase()
        .replace([' ', '-'], "_");
    let bracketed = text.starts_with(['[', '(']) && text.ends_with([']', ')']);
    bracketed && matches!(inner.as_str(), "blank_audio" | "silence" | "no_speech")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_uses_minutes_and_seconds() {
        assert_eq!(clock(0), "00:00");
        assert_eq!(clock(9_999), "00:09");
        assert_eq!(clock(61_000), "01:01");
        assert_eq!(clock(3_725_000), "62:05");
    }

    #[test]
    fn cleaning_trims_and_drops_blank_markers() {
        assert_eq!(cleaned_text("  Hello  "), Some("Hello".to_string()));
        assert_eq!(cleaned_text("नमस्ते दुनिया"), Some("नमस्ते दुनिया".to_string()));
        for blank in ["", "   ", "[BLANK_AUDIO]", " [ Silence ] ", "(silence)", "[no speech]"] {
            assert_eq!(cleaned_text(blank), None, "{blank:?}");
        }
        assert_eq!(cleaned_text("[Music]"), Some("[Music]".to_string()));
        assert_eq!(cleaned_text("silence"), Some("silence".to_string()));
    }
}
