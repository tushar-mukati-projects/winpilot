use std::borrow::Cow;

const OBJECT_REPLACEMENT: char = '\u{FFFC}';

pub(crate) fn without_object_marks(text: &str) -> Cow<'_, str> {
    if text.contains(OBJECT_REPLACEMENT) {
        Cow::Owned(text.replace(OBJECT_REPLACEMENT, ""))
    } else {
        Cow::Borrowed(text)
    }
}

pub fn readable_lines(text: &str) -> Cow<'_, str> {
    if !text.contains(OBJECT_REPLACEMENT) {
        return Cow::Borrowed(text);
    }
    let kept: Vec<String> = text
        .split('\n')
        .filter(|line| !line.contains(OBJECT_REPLACEMENT) || !line.replace(OBJECT_REPLACEMENT, "").trim().is_empty())
        .map(|line| line.replace(OBJECT_REPLACEMENT, ""))
        .collect();
    Cow::Owned(kept.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_marks_are_removed_from_names() {
        assert_eq!(without_object_marks("Send \u{FFFC}message"), "Send message");
        assert_eq!(without_object_marks("\u{FFFC}\u{FFFC}"), "");
        assert!(matches!(without_object_marks("plain"), Cow::Borrowed("plain")));
    }

    #[test]
    fn lines_made_only_of_object_marks_are_dropped() {
        let slack = "general\n\u{FFFC}\n\u{FFFC} \u{FFFC}\nhello \u{FFFC}world\n\nbye";
        assert_eq!(readable_lines(slack), "general\nhello world\n\nbye");
        assert_eq!(readable_lines("\u{FFFC}"), "");
        assert!(matches!(readable_lines("a\n\nb"), Cow::Borrowed("a\n\nb")));
    }

    #[test]
    fn carriage_returns_stay_with_their_line() {
        assert_eq!(readable_lines("one\r\n\u{FFFC}\r\ntwo"), "one\r\ntwo");
    }
}
