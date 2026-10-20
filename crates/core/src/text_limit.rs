pub const DEFAULT_MAX_CHARS: usize = 20_000;
pub(crate) const MAX_CHARS_LIMIT: usize = 200_000;
pub const ELLIPSIS: char = '…';

pub fn char_limit(requested: Option<u32>) -> usize {
    requested.map_or(DEFAULT_MAX_CHARS, |max| max as usize).clamp(1, MAX_CHARS_LIMIT)
}

pub fn truncated(mut text: String, max: usize) -> (String, bool) {
    match text.char_indices().nth(max) {
        Some((cut, _)) => {
            text.truncate(cut);
            (text, true)
        }
        None => (text, false),
    }
}

pub fn ellipsized(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((cut, _)) => format!("{}{ELLIPSIS}", &text[..cut]),
        None => text.to_string(),
    }
}

pub fn capped(text: &str, limit: usize) -> String {
    match clipped(text, limit) {
        (kept, true) => format!("{}{ELLIPSIS}", kept.trim_end()),
        (kept, false) => kept.to_string(),
    }
}

pub(crate) fn clipped(text: &str, limit: usize) -> (&str, bool) {
    if text.chars().nth(limit).is_none() {
        return (text, false);
    }
    let cut = text.char_indices().nth(limit.saturating_sub(1)).map_or(0, |(index, _)| index);
    (&text[..cut], true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_default_and_clamp() {
        assert_eq!(char_limit(None), 20_000);
        assert_eq!(char_limit(Some(0)), 1);
        assert_eq!(char_limit(Some(50)), 50);
        assert_eq!(char_limit(Some(u32::MAX)), 200_000);
    }

    #[test]
    fn truncation_respects_char_boundaries() {
        assert_eq!(truncated("héllo".to_string(), 2), ("hé".to_string(), true));
        assert_eq!(truncated("日本語".to_string(), 3), ("日本語".to_string(), false));
        assert_eq!(truncated("hi".to_string(), 5), ("hi".to_string(), false));
        assert_eq!(truncated(String::new(), 1), (String::new(), false));
    }

    #[test]
    fn ellipsized_text_keeps_max_characters_then_the_ellipsis() {
        assert_eq!(ellipsized("abcdef", 4), "abcd…");
        assert_eq!(ellipsized("abcd", 4), "abcd");
        assert_eq!(ellipsized("ééé", 2), "éé…");
    }
}
