const LONGEST_UNIT: usize = 8;
const FEWEST_REPEATS: usize = 4;
const FEWEST_LOOPED_TOKENS: usize = 12;

pub(crate) fn loop_start(tokens: &[u32]) -> Option<usize> {
    (1..=LONGEST_UNIT).find_map(|unit| {
        let repeats = trailing_repeats(tokens, unit);
        let looping = repeats >= FEWEST_REPEATS && repeats * unit >= FEWEST_LOOPED_TOKENS;
        looping.then(|| tokens.len() - (repeats - 1) * unit)
    })
}

fn trailing_repeats(tokens: &[u32], unit: usize) -> usize {
    let Some(pattern) = tokens.len().checked_sub(unit).map(|start| &tokens[start..]) else { return 0 };
    tokens.rchunks_exact(unit).take_while(|chunk| *chunk == pattern).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_syllable_repeated_over_and_over_is_cut_after_its_first_copy() {
        let mut tokens = vec![10, 11, 12];
        tokens.extend([7; 14]);
        assert_eq!(loop_start(&tokens), Some(4));
    }

    #[test]
    fn a_repeated_phrase_is_cut_after_its_first_copy() {
        let mut tokens = vec![1, 2];
        for _ in 0..5 {
            tokens.extend([20, 21, 22]);
        }
        assert_eq!(loop_start(&tokens), Some(5));
    }

    #[test]
    fn ordinary_speech_and_short_echoes_are_left_alone() {
        assert_eq!(loop_start(&[1, 2, 3, 4, 5, 6, 7, 8]), None);
        assert_eq!(loop_start(&[5, 9, 9, 9]), None);
        assert_eq!(loop_start(&[3, 4, 3, 4, 3, 4]), None);
        assert_eq!(loop_start(&[]), None);
    }
}
