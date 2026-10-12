use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use deckhand_core::Result;

use super::capture::{AudioCapture, AudioChunk};
use super::pcm::{AudioFormat, Pcm};
use super::source::AudioSource;

const POLL: Duration = Duration::from_millis(50);

pub fn capture_audio(source: AudioSource, duration: Duration, mut check: impl FnMut() -> Result<()>) -> Result<Pcm> {
    let capture = AudioCapture::start(source)?;
    let format = capture.format();
    let mut samples = Vec::with_capacity(sample_count(format, duration));
    let deadline = capture.origin().checked_add(duration).unwrap_or_else(Instant::now);
    while let Some(wait) = deadline.checked_duration_since(Instant::now()).filter(|left| !left.is_zero()) {
        check()?;
        match capture.chunks().recv_timeout(wait.min(POLL)) {
            Ok(chunk) => samples.extend(chunk.samples),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    let end = capture.stop();
    if let Some(error) = end.error {
        return Err(error);
    }
    Ok(Pcm {
        samples: exactly(samples, &end.chunks, format, duration),
        sample_rate: format.sample_rate,
        channels: format.channels,
    })
}

fn exactly(mut samples: Vec<f32>, rest: &[AudioChunk], format: AudioFormat, duration: Duration) -> Vec<f32> {
    let wanted = sample_count(format, duration);
    for chunk in rest {
        samples.extend(&chunk.samples);
    }
    samples.resize(wanted, 0.0);
    samples
}

fn sample_count(format: AudioFormat, duration: Duration) -> usize {
    let frames = duration.as_nanos() * u128::from(format.sample_rate) / 1_000_000_000;
    usize::try_from(frames).unwrap_or(usize::MAX / 2) * format.channel_count()
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEREO: AudioFormat = AudioFormat { sample_rate: 48_000, channels: 2 };

    #[test]
    fn durations_become_whole_interleaved_frames() {
        assert_eq!(sample_count(STEREO, Duration::from_secs(1)), 96_000);
        assert_eq!(sample_count(STEREO, Duration::from_micros(30)), 2);
        assert_eq!(sample_count(AudioFormat { sample_rate: 16_000, channels: 1 }, Duration::from_millis(250)), 4000);
    }

    #[test]
    fn collected_audio_is_cut_or_padded_to_the_asked_duration() {
        let rest = [AudioChunk { first_frame: 1, samples: vec![0.5; 4] }];
        let cut = exactly(vec![0.25; 2], &rest, STEREO, Duration::from_micros(50));
        assert_eq!(cut, [0.25, 0.25, 0.5, 0.5]);
        let padded = exactly(vec![0.25; 2], &[], STEREO, Duration::from_micros(50));
        assert_eq!(padded, [0.25, 0.25, 0.0, 0.0]);
    }
}
