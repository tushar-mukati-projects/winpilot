pub const DEFAULT_MAX_CHARS: usize = 20_000;
const MAX_CHARS_LIMIT: usize = 200_000;

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
}
