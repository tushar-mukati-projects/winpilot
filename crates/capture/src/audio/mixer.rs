use std::collections::VecDeque;

pub(crate) struct Mixer {
    first: VecDeque<f32>,
    second: VecDeque<f32>,
}

impl Mixer {
    pub(crate) const fn new() -> Self {
        Self { first: VecDeque::new(), second: VecDeque::new() }
    }

    pub(crate) fn push(&mut self, first: &[f32], second: &[f32]) {
        self.first.extend(first);
        self.second.extend(second);
    }

    pub(crate) fn mixed(&mut self) -> Vec<f32> {
        let common = self.first.len().min(self.second.len());
        self.take(common)
    }

    pub(crate) fn drained(&mut self) -> Vec<f32> {
        let longest = self.first.len().max(self.second.len());
        self.take(longest)
    }

    fn take(&mut self, count: usize) -> Vec<f32> {
        (0..count).map(|_| mix(self.first.pop_front(), self.second.pop_front())).collect()
    }
}

fn mix(first: Option<f32>, second: Option<f32>) -> f32 {
    (first.unwrap_or(0.0) + second.unwrap_or(0.0)).clamp(-1.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_overlap_of_both_streams_is_mixed() {
        let mut mixer = Mixer::new();
        mixer.push(&[0.1, 0.2, 0.3, 0.4], &[0.5, 0.5]);
        assert_eq!(mixer.mixed(), [0.6, 0.7]);
        assert!(mixer.mixed().is_empty());
        mixer.push(&[], &[0.1, 0.1, 0.1]);
        assert_eq!(mixer.mixed(), [0.4, 0.5]);
    }

    #[test]
    fn sums_are_clipped_to_full_scale() {
        let mut mixer = Mixer::new();
        mixer.push(&[0.8, -0.9], &[0.7, -0.6]);
        assert_eq!(mixer.mixed(), [1.0, -1.0]);
    }

    #[test]
    fn draining_pads_the_shorter_stream_with_silence() {
        let mut mixer = Mixer::new();
        mixer.push(&[0.25, 0.25, 0.25], &[0.25]);
        assert_eq!(mixer.drained(), [0.5, 0.25, 0.25]);
        assert!(mixer.drained().is_empty());
    }
}
