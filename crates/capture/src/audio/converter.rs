use super::channels::remap;
use super::pcm::AudioFormat;
use super::resample::LinearResampler;

pub(crate) struct Converter {
    from_channels: usize,
    to_channels: usize,
    resampler: Option<LinearResampler>,
}

impl Converter {
    pub(crate) fn new(from: AudioFormat, to: AudioFormat) -> Self {
        let to_channels = to.channel_count();
        let resampler = (from.sample_rate != to.sample_rate)
            .then(|| LinearResampler::new(from.sample_rate, to.sample_rate, to_channels));
        Self { from_channels: from.channel_count(), to_channels, resampler }
    }

    pub(crate) fn convert(&mut self, input: &[f32]) -> Vec<f32> {
        let remapped = remap(input, self.from_channels, self.to_channels);
        match self.resampler.as_mut() {
            Some(resampler) => resampler.process(&remapped),
            None => remapped,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn format(sample_rate: u32, channels: u16) -> AudioFormat {
        AudioFormat { sample_rate, channels }
    }

    #[test]
    fn a_matching_format_is_copied() {
        let mut converter = Converter::new(format(48_000, 2), format(48_000, 2));
        assert_eq!(converter.convert(&[0.5, -0.5]), [0.5, -0.5]);
    }

    #[test]
    fn a_mono_mic_at_another_rate_becomes_stereo_at_the_target_rate() {
        let mut converter = Converter::new(format(16_000, 1), format(48_000, 2));
        let output = converter.convert(&vec![0.25; 16_000]);
        assert!((output.len() / 2).abs_diff(48_000) <= 3, "{}", output.len() / 2);
        assert!(output.iter().skip(6).all(|sample| (sample - 0.25).abs() < 1e-6));
    }
}
