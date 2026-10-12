use super::converter::Converter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Pcm {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl AudioFormat {
    pub(crate) const fn channel_count(self) -> usize {
        if self.channels == 0 { 1 } else { self.channels as usize }
    }

    pub(crate) fn frames_in(self, samples: usize) -> usize {
        samples / self.channel_count()
    }
}

impl Pcm {
    pub const fn format(&self) -> AudioFormat {
        AudioFormat { sample_rate: self.sample_rate, channels: self.channels }
    }

    pub fn frames(&self) -> usize {
        self.format().frames_in(self.samples.len())
    }

    pub fn seconds(&self) -> f64 {
        self.frames() as f64 / f64::from(self.sample_rate.max(1))
    }

    #[must_use]
    pub fn converted(&self, format: AudioFormat) -> Self {
        let samples = Converter::new(self.format(), format).convert(&self.samples);
        Self { samples, sample_rate: format.sample_rate, channels: format.channels }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_counts_frames_and_seconds() {
        let pcm = Pcm { samples: vec![0.0; 96_000], sample_rate: 48_000, channels: 2 };
        assert_eq!(pcm.frames(), 48_000);
        assert!((pcm.seconds() - 1.0).abs() < 1e-9);
        assert_eq!(Pcm::default().frames(), 0);
        assert!(Pcm::default().seconds().abs() < f64::EPSILON);
    }

    #[test]
    fn pcm_converts_to_a_speech_friendly_format() {
        let pcm = Pcm { samples: [0.5, -0.5].repeat(48_000), sample_rate: 48_000, channels: 2 };
        let mono = pcm.converted(AudioFormat { sample_rate: 16_000, channels: 1 });
        assert_eq!((mono.sample_rate, mono.channels), (16_000, 1));
        assert!(mono.frames().abs_diff(16_000) <= 1, "{}", mono.frames());
        assert!(mono.samples.iter().all(|sample| sample.abs() < 1e-6));
    }
}
