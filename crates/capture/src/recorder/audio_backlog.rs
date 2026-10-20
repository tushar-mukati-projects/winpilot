use super::audio_feed::TRACK_FORMAT;

const TRACK_CHANNELS: usize = TRACK_FORMAT.channels as usize;
const PCM16_SCALE: f32 = 32_767.0;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TrackSamples {
    pub first_frame: u64,
    pub pcm: Vec<i16>,
}

#[derive(Debug, Default)]
pub(crate) struct TrackBacklog {
    pending: Vec<f32>,
    written: u64,
}

impl TrackBacklog {
    pub(crate) fn extend(&mut self, samples: &[f32]) {
        self.pending.extend_from_slice(samples);
    }

    pub(crate) fn reached(&self) -> u64 {
        self.written + (self.pending.len() / TRACK_CHANNELS) as u64
    }

    pub(crate) fn pad_to(&mut self, limit: u64) {
        let missing = limit.saturating_sub(self.reached());
        self.pending.resize(self.pending.len() + missing as usize * TRACK_CHANNELS, 0.0);
    }

    pub(crate) fn split_off(&mut self, limit: u64) -> Option<TrackSamples> {
        let available = (self.pending.len() / TRACK_CHANNELS) as u64;
        let frames = available.min(limit.saturating_sub(self.written));
        if frames == 0 {
            return None;
        }
        let pcm = to_pcm16(self.pending.drain(..frames as usize * TRACK_CHANNELS));
        let samples = TrackSamples { first_frame: self.written, pcm };
        self.written += frames;
        Some(samples)
    }
}

fn to_pcm16(samples: impl Iterator<Item = f32>) -> Vec<i16> {
    samples.map(|sample| (sample.clamp(-1.0, 1.0) * PCM16_SCALE).round() as i16).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_samples_become_clipped_sixteen_bit_pcm() {
        let pcm = to_pcm16([0.0, 1.0, -1.0, 0.5, 2.0, -3.0].into_iter());
        assert_eq!(pcm, [0, 32_767, -32_767, 16_384, 32_767, -32_767]);
    }

    #[test]
    fn samples_are_handed_out_only_up_to_the_video_so_far() {
        let mut backlog = TrackBacklog::default();
        backlog.extend(&[0.5; 10]);
        assert_eq!(backlog.reached(), 5);
        let first = backlog.split_off(3).unwrap();
        assert_eq!((first.first_frame, first.pcm.len()), (0, 6));
        assert_eq!(backlog.split_off(3), None);
        let rest = backlog.split_off(100).unwrap();
        assert_eq!((rest.first_frame, rest.pcm.len()), (3, 4));
        assert_eq!(backlog.reached(), 5);
    }

    #[test]
    fn padding_fills_silence_up_to_the_limit_only() {
        let mut backlog = TrackBacklog::default();
        backlog.extend(&[0.25; 4]);
        backlog.pad_to(5);
        assert_eq!(backlog.reached(), 5);
        backlog.pad_to(3);
        let all = backlog.split_off(5).unwrap();
        assert_eq!(all.pcm, [8192, 8192, 8192, 8192, 0, 0, 0, 0, 0, 0]);
    }
}
