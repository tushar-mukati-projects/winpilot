const ELLIPSIS: char = '…';
pub(crate) const NOTE_LIMIT: usize = 40;

pub(crate) fn quoted(text: &str, limit: usize) -> String {
    let mut out = String::with_capacity(text.len().min(limit * 2) + 2);
    out.push('"');
    for ch in text.chars().take(limit) {
        push_escaped(&mut out, ch);
    }
    if text.chars().nth(limit).is_some() {
        out.push(ELLIPSIS);
    }
    out.push('"');
    out
}

pub(crate) fn clipped(text: &str, limit: usize) -> String {
    let mut out: String = text.chars().take(limit).collect();
    if text.chars().nth(limit).is_some() {
        out.push(ELLIPSIS);
    }
    out
}

fn push_escaped(out: &mut String, ch: char) {
    match ch {
        '"' => out.push_str("\\\""),
        '\\' => out.push_str("\\\\"),
        '\n' => out.push_str("\\n"),
        '\r' => out.push_str("\\r"),
        '\t' => out.push_str("\\t"),
        ch if ch.is_control() => out.push(' '),
        ch => out.push(ch),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_quoted_whole() {
        assert_eq!(quoted("hello", 24), "\"hello\"");
        assert_eq!(quoted("", 24), "\"\"");
    }

    #[test]
    fn long_text_is_clipped_with_an_ellipsis() {
        assert_eq!(quoted("abcdef", 3), "\"abc…\"");
        assert_eq!(quoted("abc", 3), "\"abc\"");
        assert_eq!(clipped("abcdef", 4), "abcd…");
        assert_eq!(clipped("abcd", 4), "abcd");
    }

    #[test]
    fn clipping_counts_characters_not_bytes() {
        assert_eq!(quoted("ééééé", 2), "\"éé…\"");
    }

    #[test]
    fn quotes_and_control_characters_are_escaped() {
        assert_eq!(quoted("a\"b\\c\nd\te", 24), "\"a\\\"b\\\\c\\nd\\te\"");
        assert_eq!(quoted("x\u{7}y", 24), "\"x y\"");
    }
}
