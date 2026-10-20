use std::time::{Duration, Instant};

use crate::time_units::frames_in;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Lead {
    Pad(u64),
    Skip(u64),
}

pub(crate) fn lead_for(audio_origin: Instant, video_start: Instant, rate: u32) -> Lead {
    match audio_origin.checked_duration_since(video_start) {
        Some(late) => Lead::Pad(whole_frames(late, rate)),
        None => Lead::Skip(whole_frames(video_start.duration_since(audio_origin), rate)),
    }
}

fn whole_frames(span: Duration, rate: u32) -> u64 {
    u64::try_from(frames_in(span, rate)).unwrap_or(u64::MAX)
}

impl Lead {
    pub(crate) fn apply(&mut self, samples: Vec<f32>, channels: usize) -> Vec<f32> {
        match *self {
            Self::Pad(0) | Self::Skip(0) => samples,
            Self::Pad(frames) => {
                *self = Self::Pad(0);
                let mut padded = vec![0.0; frames as usize * channels];
                padded.extend(samples);
                padded
            }
            Self::Skip(frames) => {
                let available = (samples.len() / channels.max(1)) as u64;
                let skipped = frames.min(available);
                *self = Self::Skip(frames - skipped);
                samples[skipped as usize * channels..].to_vec()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_that_started_first_is_trimmed_to_the_video_start() {
        let origin = Instant::now();
        let video = origin + Duration::from_millis(250);
        assert_eq!(lead_for(origin, video, 48_000), Lead::Skip(12_000));
        assert_eq!(lead_for(video, origin, 48_000), Lead::Pad(12_000));
        assert_eq!(lead_for(origin, origin, 48_000), Lead::Pad(0));
    }

    #[test]
    fn skipping_spans_several_chunks() {
        let mut lead = Lead::Skip(3);
        assert_eq!(lead.apply(vec![0.1, 0.1, 0.2, 0.2], 2), Vec::<f32>::new());
        assert_eq!(lead, Lead::Skip(1));
        assert_eq!(lead.apply(vec![0.3, 0.3, 0.4, 0.4], 2), [0.4, 0.4]);
        assert_eq!(lead.apply(vec![0.5, 0.5], 2), [0.5, 0.5]);
    }

    #[test]
    fn padding_is_added_once_in_front() {
        let mut lead = Lead::Pad(2);
        assert_eq!(lead.apply(vec![0.5], 1), [0.0, 0.0, 0.5]);
        assert_eq!(lead.apply(vec![0.6], 1), [0.6]);
    }
}
