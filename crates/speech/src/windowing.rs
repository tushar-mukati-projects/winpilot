use candle_core::{Device, Tensor};
use candle_transformers::models::whisper::{Config, HOP_LENGTH, N_FRAMES, N_SAMPLES, audio::pcm_to_mel};
use winpilot_core::{Context, Result};

use crate::decoder::{SpeechModel, WindowTokens};
use crate::languages::language_code;
use crate::options::ResolvedOptions;
use crate::segments::{TIMESTAMP_STEP_MS, TimedTokens, split_window};
use crate::transcript::{Segment, Transcript, cleaned_text};
use crate::vocab::{SpecialTokens, decode_text};

const NO_SPEECH_THRESHOLD: f64 = 0.6;
const LOGPROB_THRESHOLD: f64 = -1.0;
const FRAME_MS: u64 = 10;
const CONTEXT_PER_DECODED_TOKEN: usize = 2;

pub(crate) struct TranscribeJob<'a> {
    pub filters: &'a [f32],
    pub tokens: &'a [Vec<u8>],
    pub special: &'a SpecialTokens,
    pub options: ResolvedOptions,
}

pub(crate) fn transcribe_windows(model: &mut SpeechModel, mono: &[f32], job: &TranscribeJob) -> Result<Transcript> {
    let content_frames = mono.len().div_ceil(HOP_LENGTH);
    let mel = mel_spectrogram(&model.whisper.config, mono, job.filters)?;
    let max_tokens = model.whisper.config.max_target_positions / CONTEXT_PER_DECODED_TOKEN;
    let mut language = job.options.language;
    let mut segments = Vec::new();
    let mut seek = 0;
    while seek < content_frames {
        let window = mel.narrow(2, seek, N_FRAMES).context("slicing a 30-second window")?;
        let features = model.encode(&window)?;
        let language_index = match language {
            Some(index) => Some(index),
            None if job.special.multilingual => Some(model.detect_language(&features, job.special)?),
            None => None,
        };
        language = language_index;
        let prompt = prompt(job.special, language_index, job.options.translate);
        let decoded = model.decode_window(&features, &prompt, job.special, max_tokens)?;
        let window_frames = N_FRAMES.min(content_frames - seek);
        let advance = collect_window(&decoded, seek, window_frames, content_frames, job, &mut segments);
        seek += advance.max(1);
    }
    let language = language.and_then(language_code).unwrap_or("en").to_string();
    Ok(Transcript { language, segments })
}

fn mel_spectrogram(config: &Config, mono: &[f32], filters: &[f32]) -> Result<Tensor> {
    let mut padded = Vec::with_capacity(mono.len() + N_SAMPLES);
    padded.extend_from_slice(mono);
    padded.resize(mono.len() + N_SAMPLES, 0.0);
    let mel = pcm_to_mel(config, &padded, filters);
    let bins = config.num_mel_bins;
    let frames = mel.len() / bins;
    Tensor::from_vec(mel, (1, bins, frames), &Device::Cpu).context("building the mel spectrogram")
}

fn prompt(special: &SpecialTokens, language: Option<usize>, translate: bool) -> Vec<u32> {
    if !special.multilingual {
        return vec![special.sot];
    }
    let mut prompt = vec![special.sot];
    prompt.extend(language.and_then(|index| special.language_token(index)));
    prompt.push(if translate { special.translate } else { special.transcribe });
    prompt
}

fn collect_window(
    decoded: &WindowTokens,
    seek: usize,
    window_frames: usize,
    content_frames: usize,
    job: &TranscribeJob,
    segments: &mut Vec<Segment>,
) -> usize {
    if decoded.no_speech_probability > NO_SPEECH_THRESHOLD && decoded.average_logprob < LOGPROB_THRESHOLD {
        return window_frames;
    }
    let split = split_window(&decoded.tokens, job.special);
    let offset_ms = seek as u64 * FRAME_MS;
    let window_end_ms = offset_ms + window_frames as u64 * FRAME_MS;
    let content_end_ms = content_frames as u64 * FRAME_MS;
    for timed in &split.segments {
        if let Some(segment) = to_segment(timed, offset_ms, window_end_ms.min(content_end_ms), job.tokens) {
            segments.push(segment);
        }
    }
    let resumed = split.resume_at.map(|step| step as usize * (TIMESTAMP_STEP_MS / FRAME_MS) as usize);
    resumed.filter(|frames| *frames < window_frames).unwrap_or(window_frames)
}

fn to_segment(timed: &TimedTokens, offset_ms: u64, limit_ms: u64, tokens: &[Vec<u8>]) -> Option<Segment> {
    let text = cleaned_text(&decode_text(tokens, &timed.text))?;
    let start_ms = (offset_ms + u64::from(timed.start) * TIMESTAMP_STEP_MS).min(limit_ms);
    let end_ms =
        timed.end.map_or(limit_ms, |end| offset_ms + u64::from(end) * TIMESTAMP_STEP_MS).clamp(start_ms, limit_ms);
    Some(Segment { start_ms, end_ms, text })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn special() -> SpecialTokens {
        SpecialTokens::for_vocab_size(51_865)
    }

    #[test]
    fn prompts_name_the_language_and_task() {
        let special = special();
        assert_eq!(prompt(&special, Some(17), false), vec![special.sot, special.sot + 18, special.transcribe]);
        assert_eq!(prompt(&special, None, true), vec![special.sot, special.translate]);
        let english = SpecialTokens::for_vocab_size(51_864);
        assert_eq!(prompt(&english, Some(0), false), vec![english.sot]);
    }

    #[test]
    fn segments_are_offset_and_clamped() {
        let tokens = vec![b" Hello".to_vec(), b" world".to_vec()];
        let timed = TimedTokens { start: 50, end: Some(2_000), text: vec![0, 1] };
        let segment = to_segment(&timed, 30_000, 45_000, &tokens).unwrap();
        assert_eq!(segment, Segment { start_ms: 31_000, end_ms: 45_000, text: "Hello world".to_string() });
        let open = TimedTokens { start: 0, end: None, text: vec![0] };
        assert_eq!(to_segment(&open, 0, 12_000, &tokens).unwrap().end_ms, 12_000);
        let blank = TimedTokens { start: 0, end: Some(10), text: vec![] };
        assert_eq!(to_segment(&blank, 0, 12_000, &tokens), None);
    }
}
