pub(crate) const STEREO_48_KHZ: AudioFormat = AudioFormat { sample_rate: 48_000, channels: 2 };

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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_counts_frames() {
        let pcm = Pcm { samples: vec![0.0; 96_000], sample_rate: 48_000, channels: 2 };
        assert_eq!(pcm.frames(), 48_000);
        assert_eq!(Pcm::default().frames(), 0);
    }
}
