use crate::vocab::SpecialTokens;

const MAX_INITIAL_TIMESTAMP_STEPS: usize = 50;

pub(crate) fn apply_rules(logits: &mut [f32], sampled: &[u32], special: &SpecialTokens) {
    suppress_control_tokens(logits, special);
    if sampled.is_empty() {
        force_initial_timestamp(logits, special);
    } else {
        pair_timestamps(logits, sampled, special);
        keep_timestamps_increasing(logits, sampled, special);
    }
    prefer_timestamp_when_likely(logits, special);
}

pub(crate) fn greedy(logits: &[f32]) -> Option<(u32, f64)> {
    let (index, best) = logits
        .iter()
        .enumerate()
        .filter(|(_, value)| value.is_finite())
        .max_by(|left, right| left.1.total_cmp(right.1))?;
    let logprob = f64::from(*best) - log_sum_exp(logits);
    Some((u32::try_from(index).ok()?, logprob))
}

pub(crate) fn log_sum_exp(values: &[f32]) -> f64 {
    let peak = values.iter().copied().filter(|value| value.is_finite()).fold(f32::NEG_INFINITY, f32::max);
    if !peak.is_finite() {
        return f64::NEG_INFINITY;
    }
    let total: f64 = values.iter().map(|value| f64::from(value - peak).exp()).sum();
    f64::from(peak) + total.ln()
}

fn suppress(logits: &mut [f32], range: std::ops::Range<usize>) {
    let end = range.end.min(logits.len());
    let start = range.start.min(end);
    logits[start..end].fill(f32::NEG_INFINITY);
}

fn suppress_one(logits: &mut [f32], token: u32) {
    if let Some(value) = logits.get_mut(token as usize) {
        *value = f32::NEG_INFINITY;
    }
}

fn suppress_control_tokens(logits: &mut [f32], special: &SpecialTokens) {
    let controls = [
        special.sot,
        special.translate,
        special.transcribe,
        special.start_of_lm,
        special.previous,
        special.no_speech,
        special.no_timestamps,
    ];
    for token in controls {
        suppress_one(logits, token);
    }
    let first_language = special.sot as usize + 1;
    suppress(logits, first_language..first_language + special.language_count);
}

fn force_initial_timestamp(logits: &mut [f32], special: &SpecialTokens) {
    let begin = special.timestamp_begin as usize;
    suppress(logits, 0..begin);
    suppress(logits, begin + MAX_INITIAL_TIMESTAMP_STEPS + 1..logits.len());
}

fn pair_timestamps(logits: &mut [f32], sampled: &[u32], special: &SpecialTokens) {
    let last_was_timestamp = sampled.last().is_some_and(|token| special.is_timestamp(*token));
    let penultimate_was_timestamp = sampled.len() < 2 || special.is_timestamp(sampled[sampled.len() - 2]);
    if !last_was_timestamp {
        return;
    }
    if penultimate_was_timestamp {
        suppress(logits, special.timestamp_begin as usize..logits.len());
    } else {
        suppress(logits, 0..special.eot as usize);
    }
}

fn keep_timestamps_increasing(logits: &mut [f32], sampled: &[u32], special: &SpecialTokens) {
    let Some(last_timestamp) = sampled.iter().rev().copied().find(|token| special.is_timestamp(*token)) else {
        return;
    };
    let closes_segment = sampled.len() >= 2
        && special.is_timestamp(sampled[sampled.len() - 1])
        && !special.is_timestamp(sampled[sampled.len() - 2]);
    let floor = if closes_segment { last_timestamp } else { last_timestamp + 1 };
    suppress(logits, special.timestamp_begin as usize..floor as usize);
}

fn prefer_timestamp_when_likely(logits: &mut [f32], special: &SpecialTokens) {
    let begin = (special.timestamp_begin as usize).min(logits.len());
    let total = log_sum_exp(logits);
    let timestamp_mass = log_sum_exp(&logits[begin..]) - total;
    let best_text = logits[..begin].iter().copied().fold(f32::NEG_INFINITY, f32::max);
    if timestamp_mass > f64::from(best_text) - total {
        suppress(logits, 0..begin);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VOCAB: usize = 51_865;

    fn special() -> SpecialTokens {
        SpecialTokens::for_vocab_size(VOCAB)
    }

    fn flat() -> Vec<f32> {
        vec![0.0; VOCAB]
    }

    fn allowed(logits: &[f32], token: u32) -> bool {
        logits[token as usize].is_finite()
    }

    #[test]
    fn first_token_must_be_an_early_timestamp() {
        let special = special();
        let mut logits = flat();
        apply_rules(&mut logits, &[], &special);
        assert!(!allowed(&logits, 100));
        assert!(!allowed(&logits, special.eot));
        assert!(allowed(&logits, special.timestamp_begin));
        assert!(allowed(&logits, special.timestamp_begin + 50));
        assert!(!allowed(&logits, special.timestamp_begin + 51));
    }

    #[test]
    fn control_and_language_tokens_are_never_sampled() {
        let special = special();
        let mut logits = flat();
        apply_rules(&mut logits, &[special.timestamp_begin, 100], &special);
        for token in [special.sot, special.transcribe, special.no_timestamps, special.sot + 18] {
            assert!(!allowed(&logits, token), "{token}");
        }
    }

    #[test]
    fn text_follows_an_opening_timestamp() {
        let special = special();
        let mut logits = flat();
        logits[200] = 5.0;
        apply_rules(&mut logits, &[special.timestamp_begin + 3], &special);
        assert!(allowed(&logits, 200));
        assert!(!allowed(&logits, special.timestamp_begin + 10));
    }

    #[test]
    fn a_closing_timestamp_is_followed_by_a_timestamp_or_the_end() {
        let special = special();
        let mut logits = flat();
        apply_rules(&mut logits, &[special.timestamp_begin, 100, special.timestamp_begin + 20], &special);
        assert!(!allowed(&logits, 100));
        assert!(allowed(&logits, special.eot) || allowed(&logits, special.timestamp_begin + 20));
        assert!(allowed(&logits, special.timestamp_begin + 20));
        assert!(!allowed(&logits, special.timestamp_begin + 19));
    }

    #[test]
    fn timestamps_never_go_backwards_inside_a_segment() {
        let special = special();
        let mut logits = flat();
        logits[100] = 10.0;
        apply_rules(&mut logits, &[special.timestamp_begin + 30, 100, 101], &special);
        assert!(!allowed(&logits, special.timestamp_begin + 30));
        assert!(allowed(&logits, 100));
    }

    #[test]
    fn likely_timestamps_win_over_text() {
        let special = special();
        let mut logits = vec![-10.0; VOCAB];
        logits[special.timestamp_begin as usize + 40] = 8.0;
        logits[100] = 7.0;
        apply_rules(&mut logits, &[special.timestamp_begin, 100], &special);
        assert!(!allowed(&logits, 100));
        assert_eq!(greedy(&logits).unwrap().0, special.timestamp_begin + 40);
    }

    #[test]
    fn greedy_picks_the_best_finite_token_with_its_logprob() {
        let (token, logprob) = greedy(&[0.0, 2.0, f32::NEG_INFINITY]).unwrap();
        assert_eq!(token, 1);
        assert!((logprob - (2.0 - (1.0_f64 + 2.0_f64.exp()).ln())).abs() < 1e-9);
        assert_eq!(greedy(&[f32::NEG_INFINITY]), None);
    }
}
