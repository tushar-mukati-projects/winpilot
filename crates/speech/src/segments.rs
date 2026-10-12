use crate::vocab::SpecialTokens;

pub(crate) const TIMESTAMP_STEP_MS: u64 = 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TimedTokens {
    pub start: u32,
    pub end: Option<u32>,
    pub text: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WindowSplit {
    pub segments: Vec<TimedTokens>,
    pub resume_at: Option<u32>,
}

pub(crate) fn split_window(tokens: &[u32], special: &SpecialTokens) -> WindowSplit {
    let mut segments = Vec::new();
    let mut start = None;
    let mut text = Vec::new();
    let mut last_end = None;
    for &token in tokens {
        if special.is_timestamp(token) {
            let step = token - special.timestamp_begin;
            match start {
                Some(opened) if !text.is_empty() => {
                    segments.push(TimedTokens { start: opened, end: Some(step), text: std::mem::take(&mut text) });
                    last_end = Some(step);
                    start = None;
                }
                _ => start = Some(step),
            }
        } else if special.is_text(token) {
            text.push(token);
        }
    }
    finish(segments, start, text, last_end)
}

fn finish(mut segments: Vec<TimedTokens>, start: Option<u32>, text: Vec<u32>, last_end: Option<u32>) -> WindowSplit {
    let unfinished = !text.is_empty() || start.is_some();
    if let (true, Some(end)) = (unfinished, last_end.filter(|end| *end > 0)) {
        return WindowSplit { segments, resume_at: Some(end) };
    }
    if !text.is_empty() {
        segments.push(TimedTokens { start: start.unwrap_or(0), end: None, text });
    }
    WindowSplit { segments, resume_at: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn special() -> SpecialTokens {
        SpecialTokens::for_vocab_size(51_865)
    }

    fn ts(step: u32) -> u32 {
        special().timestamp_begin + step
    }

    #[test]
    fn closed_segments_consume_the_window() {
        let split = split_window(&[ts(0), 10, 11, ts(120), ts(120), 12, ts(250), special().eot], &special());
        assert_eq!(
            split.segments,
            vec![
                TimedTokens { start: 0, end: Some(120), text: vec![10, 11] },
                TimedTokens { start: 120, end: Some(250), text: vec![12] },
            ]
        );
        assert_eq!(split.resume_at, None);
    }

    #[test]
    fn unfinished_text_after_a_closed_segment_resumes_there() {
        let split = split_window(&[ts(0), 10, ts(900), ts(900), 11, 12], &special());
        assert_eq!(split.segments.len(), 1);
        assert_eq!(split.resume_at, Some(900));
    }

    #[test]
    fn a_dangling_start_resumes_at_the_last_end() {
        let split = split_window(&[ts(0), 10, ts(700), ts(700)], &special());
        assert_eq!(split.segments.len(), 1);
        assert_eq!(split.resume_at, Some(700));
    }

    #[test]
    fn text_without_any_closing_timestamp_runs_to_the_window_end() {
        let split = split_window(&[ts(40), 10, 11], &special());
        assert_eq!(split.segments, vec![TimedTokens { start: 40, end: None, text: vec![10, 11] }]);
        assert_eq!(split.resume_at, None);
    }

    #[test]
    fn empty_and_timestamp_only_windows_have_no_segments() {
        assert_eq!(split_window(&[], &special()), WindowSplit { segments: Vec::new(), resume_at: None });
        assert_eq!(split_window(&[ts(0)], &special()).segments, Vec::new());
    }

    #[test]
    fn a_zero_resume_point_never_stalls() {
        let split = split_window(&[ts(0), ts(0), 10], &special());
        assert_eq!(split.resume_at, None);
        assert_eq!(split.segments, vec![TimedTokens { start: 0, end: None, text: vec![10] }]);
    }
}
