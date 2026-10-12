pub(crate) struct LinearResampler {
    step: f64,
    channels: usize,
    previous: Vec<f32>,
    position: f64,
}

impl LinearResampler {
    pub(crate) fn new(from_rate: u32, to_rate: u32, channels: usize) -> Self {
        let channels = channels.max(1);
        Self {
            step: f64::from(from_rate.max(1)) / f64::from(to_rate.max(1)),
            channels,
            previous: vec![0.0; channels],
            position: 1.0,
        }
    }

    pub(crate) fn process(&mut self, input: &[f32]) -> Vec<f32> {
        let frames = input.len() / self.channels;
        if frames == 0 {
            return Vec::new();
        }
        let estimate = (frames as f64 / self.step).ceil() as usize + 1;
        let mut output = Vec::with_capacity(estimate * self.channels);
        while self.position < frames as f64 {
            let index = self.position.floor() as usize;
            let fraction = (self.position - index as f64) as f32;
            for channel in 0..self.channels {
                let before = self.sample(input, index, channel);
                let after = self.sample(input, index + 1, channel);
                output.push((after - before).mul_add(fraction, before));
            }
            self.position += self.step;
        }
        self.position -= frames as f64;
        let last = (frames - 1) * self.channels;
        self.previous.copy_from_slice(&input[last..last + self.channels]);
        output
    }

    fn sample(&self, input: &[f32], virtual_frame: usize, channel: usize) -> f32 {
        match virtual_frame {
            0 => self.previous[channel],
            frame => input[(frame - 1) * self.channels + channel],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_rates_pass_frames_through_one_frame_behind() {
        let mut resampler = LinearResampler::new(48_000, 48_000, 2);
        assert_eq!(resampler.process(&[0.1, 0.2, 0.3, 0.4, 0.5, 0.6]), [0.1, 0.2, 0.3, 0.4]);
        assert_eq!(resampler.process(&[0.7, 0.8]), [0.5, 0.6]);
    }

    #[test]
    fn upsampling_interpolates_between_frames() {
        let mut resampler = LinearResampler::new(1, 2, 1);
        assert_eq!(resampler.process(&[0.0, 1.0, 0.0]), [0.0, 0.5, 1.0, 0.5]);
        assert_eq!(resampler.process(&[1.0]), [0.0, 0.5]);
    }

    #[test]
    fn downsampling_keeps_every_other_frame() {
        let mut resampler = LinearResampler::new(2, 1, 1);
        assert_eq!(resampler.process(&[0.0, 0.1, 0.2, 0.3, 0.4]), [0.0, 0.2]);
        assert_eq!(resampler.process(&[0.5, 0.6]), [0.4]);
    }

    #[test]
    fn long_streams_keep_the_rate_ratio_across_chunks() {
        let mut resampler = LinearResampler::new(44_100, 48_000, 2);
        let chunk = vec![0.25; 441 * 2];
        let produced: usize = (0..1000).map(|_| resampler.process(&chunk).len()).sum();
        assert!((produced / 2).abs_diff(480_000) <= 2, "{}", produced / 2);
    }

    #[test]
    fn empty_input_produces_nothing() {
        let mut resampler = LinearResampler::new(48_000, 16_000, 1);
        assert!(resampler.process(&[]).is_empty());
    }
}
