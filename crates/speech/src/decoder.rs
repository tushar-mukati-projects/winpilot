use candle_core::Tensor;
use candle_transformers::models::whisper::model::Whisper;
use winpilot_core::{Context, Error, Result};

use crate::logits::{apply_rules, greedy, log_sum_exp};
use crate::text_decoder::{CachedDecoder, last_position};
use crate::vocab::SpecialTokens;

pub(crate) struct SpeechModel {
    pub whisper: Whisper,
    pub decoder: CachedDecoder,
}

pub(crate) struct WindowTokens {
    pub tokens: Vec<u32>,
    pub average_logprob: f64,
    pub no_speech_probability: f64,
}

impl SpeechModel {
    pub(crate) fn encode(&mut self, mel_window: &Tensor) -> Result<Tensor> {
        self.whisper.encoder.forward(mel_window, true).context("running the speech encoder")
    }

    pub(crate) fn detect_language(&mut self, features: &Tensor, special: &SpecialTokens) -> Result<usize> {
        self.decoder.reset();
        let logits = self.step(&[special.sot], features)?;
        let first = special.sot as usize + 1;
        let candidates = logits
            .get(first..first + special.language_count)
            .ok_or_else(|| Error::internal("the speech model has fewer outputs than languages"))?;
        candidates
            .iter()
            .enumerate()
            .max_by(|left, right| left.1.total_cmp(right.1))
            .map(|(index, _)| index)
            .ok_or_else(|| Error::internal("the speech model has no language tokens"))
    }

    pub(crate) fn decode_window(
        &mut self,
        features: &Tensor,
        prompt: &[u32],
        special: &SpecialTokens,
        max_tokens: usize,
    ) -> Result<WindowTokens> {
        self.decoder.reset();
        let hidden = self.decoder.forward(prompt, features).context("running the speech decoder")?;
        let no_speech_probability = self.probability_at(&hidden, 0, special.no_speech)?;
        let mut logits = self.logits_at(&hidden, last_position(&hidden).context("reading the decoder length")?)?;
        let mut sampled = Vec::new();
        let mut logprob_sum = 0.0;
        while sampled.len() < max_tokens && self.decoder.capacity_left() > 0 {
            apply_rules(&mut logits, &sampled, special);
            let Some((token, logprob)) = greedy(&logits) else { break };
            logprob_sum += logprob;
            if token == special.eot {
                break;
            }
            sampled.push(token);
            logits = self.step(&[token], features)?;
        }
        let average_logprob = logprob_sum / (sampled.len() + 1) as f64;
        Ok(WindowTokens { tokens: sampled, average_logprob, no_speech_probability })
    }

    fn step(&mut self, tokens: &[u32], features: &Tensor) -> Result<Vec<f32>> {
        let hidden = self.decoder.forward(tokens, features).context("running the speech decoder")?;
        self.logits_at(&hidden, last_position(&hidden).context("reading the decoder length")?)
    }

    fn logits_at(&self, hidden: &Tensor, position: usize) -> Result<Vec<f32>> {
        self.decoder.logits_at(hidden, position).context("reading the decoder output")
    }

    fn probability_at(&self, hidden: &Tensor, position: usize, token: u32) -> Result<f64> {
        let logits = self.logits_at(hidden, position)?;
        let total = log_sum_exp(&logits);
        Ok(logits.get(token as usize).map_or(0.0, |value| (f64::from(*value) - total).exp()))
    }
}
