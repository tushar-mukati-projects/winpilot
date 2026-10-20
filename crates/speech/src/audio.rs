use std::f64::consts::PI;

use winpilot_core::{Error, Result};

pub(crate) const WHISPER_SAMPLE_RATE: u32 = 16_000;
pub(crate) const MIN_SAMPLE_RATE: u32 = 1_000;
pub(crate) const MAX_SAMPLE_RATE: u32 = 768_000;
const ZERO_CROSSINGS: f64 = 16.0;
const ROLLOFF: f64 = 0.94;
const TABLE_STEPS_PER_SAMPLE: f64 = 256.0;

pub(crate) fn to_whisper_input(samples: &[f32], sample_rate: u32, channels: u16) -> Result<Vec<f32>> {
    check_format(samples.len(), sample_rate, channels)?;
    let mono = downmix(samples, channels);
    Ok(resample(&mono, sample_rate, WHISPER_SAMPLE_RATE))
}

pub(crate) fn check_format(sample_count: usize, sample_rate: u32, channels: u16) -> Result<()> {
    if channels == 0 {
        return Err(Error::invalid("audio must have at least one channel"));
    }
    if !(MIN_SAMPLE_RATE..=MAX_SAMPLE_RATE).contains(&sample_rate) {
        return Err(Error::invalid(format!(
            "sample rate {sample_rate} Hz is outside {MIN_SAMPLE_RATE}-{MAX_SAMPLE_RATE} Hz"
        )));
    }
    if !sample_count.is_multiple_of(usize::from(channels)) {
        return Err(Error::invalid(format!(
            "{sample_count} interleaved samples do not divide into whole frames of {channels} channels"
        )));
    }
    Ok(())
}

fn downmix(samples: &[f32], channels: u16) -> Vec<f32> {
    let width = usize::from(channels.max(1));
    if width == 1 {
        return samples.to_vec();
    }
    let scale = 1.0 / width as f32;
    samples.chunks_exact(width).map(|frame| frame.iter().sum::<f32>() * scale).collect()
}

fn resample(mono: &[f32], from: u32, to: u32) -> Vec<f32> {
    if from == to || mono.is_empty() || from == 0 || to == 0 {
        return mono.to_vec();
    }
    let kernel = SincKernel::new(from, to);
    let ratio = f64::from(to) / f64::from(from);
    let length = (mono.len() as f64 * ratio).ceil() as usize;
    (0..length).map(|index| kernel.sample_at(mono, index as f64 / ratio)).collect()
}

struct SincKernel {
    half_width: f64,
    table: Vec<f64>,
}

impl SincKernel {
    fn new(from: u32, to: u32) -> Self {
        let cutoff = 0.5 * (f64::from(to) / f64::from(from)).min(1.0) * ROLLOFF;
        let half_width = ZERO_CROSSINGS / (2.0 * cutoff);
        let steps = (half_width * TABLE_STEPS_PER_SAMPLE).ceil() as usize;
        let table = (0..=steps + 1)
            .map(|step| windowed_sinc(step as f64 / TABLE_STEPS_PER_SAMPLE, cutoff, half_width))
            .collect();
        Self { half_width, table }
    }

    fn sample_at(&self, input: &[f32], center: f64) -> f32 {
        let first = (center - self.half_width).ceil().max(0.0) as usize;
        let last = ((center + self.half_width).floor().max(0.0) as usize).min(input.len().saturating_sub(1));
        if first > last {
            return 0.0;
        }
        let mut sum = 0.0;
        let mut weight_sum = 0.0;
        for (offset, value) in input[first..=last].iter().enumerate() {
            let weight = self.weight(center - (first + offset) as f64);
            sum += f64::from(*value) * weight;
            weight_sum += weight;
        }
        if weight_sum.abs() < f64::EPSILON { 0.0 } else { (sum / weight_sum) as f32 }
    }

    fn weight(&self, distance: f64) -> f64 {
        let position = distance.abs() * TABLE_STEPS_PER_SAMPLE;
        let index = position.floor() as usize;
        let (Some(low), Some(high)) = (self.table.get(index), self.table.get(index + 1)) else { return 0.0 };
        let fraction = position - index as f64;
        low + (high - low) * fraction
    }
}

fn windowed_sinc(distance: f64, cutoff: f64, half_width: f64) -> f64 {
    let position = distance / half_width;
    if position.abs() >= 1.0 {
        return 0.0;
    }
    sinc(2.0 * cutoff * distance) * blackman(position)
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 { 1.0 } else { (PI * x).sin() / (PI * x) }
}

fn blackman(position: f64) -> f64 {
    let phase = PI * (position + 1.0);
    0.42 - 0.5 * phase.cos() + 0.08 * (2.0 * phase).cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(frequency: f64, rate: u32, seconds: f64) -> Vec<f32> {
        let count = (f64::from(rate) * seconds) as usize;
        (0..count).map(|index| (2.0 * PI * frequency * index as f64 / f64::from(rate)).sin() as f32).collect()
    }

    fn rms(samples: &[f32]) -> f64 {
        let trimmed = &samples[samples.len() / 10..samples.len() * 9 / 10];
        (trimmed.iter().map(|value| f64::from(*value).powi(2)).sum::<f64>() / trimmed.len() as f64).sqrt()
    }

    fn zero_crossings(samples: &[f32]) -> usize {
        samples.windows(2).filter(|pair| (pair[0] < 0.0) != (pair[1] < 0.0)).count()
    }

    #[test]
    fn downmix_averages_each_frame() {
        assert_eq!(downmix(&[1.0, 0.0, 0.5, 0.5, -1.0, 1.0], 2), vec![0.5, 0.5, 0.0]);
        assert_eq!(downmix(&[0.3, 0.6, 0.9], 3), vec![0.6]);
        assert_eq!(downmix(&[0.25, -0.25], 1), vec![0.25, -0.25]);
    }

    #[test]
    fn same_rate_is_a_copy() {
        let input = vec![0.1, 0.2, 0.3];
        assert_eq!(resample(&input, 16_000, 16_000), input);
        assert!(resample(&[], 48_000, 16_000).is_empty());
    }

    #[test]
    fn downsampling_keeps_length_dc_and_tone() {
        let constant = resample(&vec![0.5; 4_800], 48_000, 16_000);
        assert_eq!(constant.len(), 1_600);
        assert!(constant.iter().all(|value| (value - 0.5).abs() < 1e-3));
        let output = resample(&tone(1_000.0, 48_000, 1.0), 48_000, 16_000);
        assert_eq!(output.len(), 16_000);
        assert!((rms(&output) - std::f64::consts::FRAC_1_SQRT_2).abs() < 0.02);
        assert!(zero_crossings(&output).abs_diff(2_000) <= 2);
    }

    #[test]
    fn downsampling_removes_tones_above_the_new_nyquist() {
        let output = resample(&tone(12_000.0, 48_000, 1.0), 48_000, 16_000);
        assert!(rms(&output) < 0.01);
    }

    #[test]
    fn upsampling_keeps_the_tone() {
        let output = resample(&tone(440.0, 8_000, 1.0), 8_000, 16_000);
        assert_eq!(output.len(), 16_000);
        assert!((rms(&output) - std::f64::consts::FRAC_1_SQRT_2).abs() < 0.02);
        assert!(zero_crossings(&output).abs_diff(880) <= 2);
    }

    #[test]
    fn odd_ratios_produce_the_expected_length() {
        assert_eq!(resample(&vec![0.0; 44_100], 44_100, 16_000).len(), 16_000);
        assert_eq!(resample(&vec![0.0; 22_050], 22_050, 16_000).len(), 16_000);
    }

    #[test]
    fn whisper_input_downmixes_then_resamples() {
        let stereo: Vec<f32> = tone(1_000.0, 48_000, 0.5).into_iter().flat_map(|value| [value, value]).collect();
        let output = to_whisper_input(&stereo, 48_000, 2).unwrap();
        assert_eq!(output.len(), 8_000);
    }

    #[test]
    fn bad_formats_are_rejected() {
        assert!(to_whisper_input(&[0.0; 4], 48_000, 0).is_err());
        assert!(to_whisper_input(&[0.0; 4], 0, 1).is_err());
        assert!(to_whisper_input(&[0.0; 4], 1_000_000, 1).is_err());
        assert!(to_whisper_input(&[0.0; 3], 48_000, 2).is_err());
        assert!(to_whisper_input(&[0.0; 4], 48_000, 2).is_ok());
    }
}
