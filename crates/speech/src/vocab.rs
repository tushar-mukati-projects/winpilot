const ENGLISH_ONLY_EOT: u32 = 50_256;
const ENGLISH_ONLY_TRANSLATE: u32 = 50_357;
const MULTILINGUAL_VOCAB: usize = 51_865;
const VOCAB_WITHOUT_LANGUAGES: usize = 51_765;
const BASE_LANGUAGE_COUNT: usize = 98;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SpecialTokens {
    pub eot: u32,
    pub sot: u32,
    pub translate: u32,
    pub transcribe: u32,
    pub start_of_lm: u32,
    pub previous: u32,
    pub no_speech: u32,
    pub no_timestamps: u32,
    pub timestamp_begin: u32,
    pub language_count: usize,
    pub multilingual: bool,
}

impl SpecialTokens {
    pub fn for_vocab_size(n_vocab: usize) -> Self {
        if n_vocab < MULTILINGUAL_VOCAB {
            return Self::layout(ENGLISH_ONLY_EOT, ENGLISH_ONLY_TRANSLATE, 0, false);
        }
        let language_count = n_vocab - VOCAB_WITHOUT_LANGUAGES - 1;
        let shift = language_count.saturating_sub(BASE_LANGUAGE_COUNT) as u32;
        Self::layout(ENGLISH_ONLY_EOT + 1, ENGLISH_ONLY_TRANSLATE + shift, language_count, true)
    }

    fn layout(eot: u32, translate: u32, language_count: usize, multilingual: bool) -> Self {
        Self {
            eot,
            sot: eot + 1,
            translate,
            transcribe: translate + 1,
            start_of_lm: translate + 2,
            previous: translate + 3,
            no_speech: translate + 4,
            no_timestamps: translate + 5,
            timestamp_begin: translate + 6,
            language_count,
            multilingual,
        }
    }

    pub fn language_token(&self, index: usize) -> Option<u32> {
        (self.multilingual && index < self.language_count).then(|| self.sot + 1 + index as u32)
    }

    pub fn is_timestamp(&self, token: u32) -> bool {
        token >= self.timestamp_begin
    }

    pub fn is_text(&self, token: u32) -> bool {
        token < self.eot
    }
}

pub(crate) fn decode_text(tokens: &[Vec<u8>], ids: &[u32]) -> String {
    let bytes: Vec<u8> = ids.iter().filter_map(|id| tokens.get(*id as usize)).flatten().copied().collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn multilingual_layout_matches_whisper_cpp() {
        let tokens = SpecialTokens::for_vocab_size(51_865);
        assert!(tokens.multilingual);
        assert_eq!(tokens.language_count, 99);
        assert_eq!((tokens.eot, tokens.sot, tokens.translate, tokens.transcribe), (50_257, 50_258, 50_358, 50_359));
        assert_eq!((tokens.start_of_lm, tokens.previous, tokens.no_speech), (50_360, 50_361, 50_362));
        assert_eq!((tokens.no_timestamps, tokens.timestamp_begin), (50_363, 50_364));
        assert_eq!(tokens.language_token(0), Some(50_259));
        assert_eq!(tokens.language_token(17), Some(50_276));
        assert_eq!(tokens.language_token(99), None);
    }

    #[test]
    fn large_v3_layout_has_one_more_language() {
        let tokens = SpecialTokens::for_vocab_size(51_866);
        assert_eq!(tokens.language_count, 100);
        assert_eq!((tokens.eot, tokens.sot, tokens.translate), (50_257, 50_258, 50_359));
        assert_eq!(tokens.timestamp_begin, 50_365);
        assert_eq!(tokens.language_token(99), Some(50_358));
    }

    #[test]
    fn english_only_layout_has_no_language_tokens() {
        let tokens = SpecialTokens::for_vocab_size(51_864);
        assert!(!tokens.multilingual);
        assert_eq!((tokens.eot, tokens.sot, tokens.translate), (50_256, 50_257, 50_357));
        assert_eq!(tokens.timestamp_begin, 50_363);
        assert_eq!(tokens.language_token(0), None);
    }

    #[test]
    fn token_classes() {
        let tokens = SpecialTokens::for_vocab_size(51_865);
        assert!(tokens.is_text(50_256));
        assert!(!tokens.is_text(tokens.eot));
        assert!(tokens.is_timestamp(50_364));
        assert!(!tokens.is_timestamp(50_363));
    }

    #[test]
    fn text_joins_bytes_across_tokens() {
        let vocab = vec![b" Hi".to_vec(), vec![0xE0, 0xA4], vec![0xA8]];
        assert_eq!(decode_text(&vocab, &[0, 1, 2]), " Hiन");
        assert_eq!(decode_text(&vocab, &[0, 99]), " Hi");
    }
}
