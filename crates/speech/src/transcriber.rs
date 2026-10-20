use std::path::Path;
use std::sync::Mutex;

use candle_core::{DType, Device};
use candle_nn::VarBuilder;
use candle_transformers::models::whisper::{Config, model::Whisper};
use winpilot_core::{Error, Result};

use crate::audio::to_whisper_input;
use crate::decoder::SpeechModel;
use crate::ggml::{Hparams, read_model};
use crate::languages::language_code;
use crate::options::{ResolvedOptions, TranscribeOptions, logical_cores};
use crate::paths::model_missing_error;
use crate::priority::run_below_normal;
use crate::text_decoder::CachedDecoder;
use crate::transcript::Transcript;
use crate::vocab::SpecialTokens;
use crate::windowing::{TranscribeJob, transcribe_windows};

const SILENCE_PEAK: f32 = 1e-3;

pub struct Transcriber {
    model: Mutex<SpeechModel>,
    filters: Vec<f32>,
    tokens: Vec<Vec<u8>>,
    special: SpecialTokens,
}

impl Transcriber {
    pub fn load(model: &Path) -> Result<Self> {
        if !model.is_file() {
            return Err(model_missing_error());
        }
        let ggml = read_model(model)?;
        let config = config_from(&ggml.hparams)?;
        let special = SpecialTokens::for_vocab_size(config.vocab_size);
        let weights = VarBuilder::from_tensors(ggml.tensors, DType::F32, &Device::Cpu);
        let unusable =
            |error| Error::unsupported(format!("{} is not a usable Whisper model: {error}", model.display()));
        let decoder = CachedDecoder::load(&weights, &config).map_err(unusable)?;
        let whisper = Whisper::load(&weights, config).map_err(unusable)?;
        let model = SpeechModel { whisper, decoder };
        Ok(Self { model: Mutex::new(model), filters: ggml.filters, tokens: ggml.tokens, special })
    }

    pub fn transcribe(
        &self,
        samples: &[f32],
        sample_rate: u32,
        channels: u16,
        options: &TranscribeOptions,
    ) -> Result<Transcript> {
        let resolved = options.resolve(logical_cores())?;
        self.check_supported(resolved)?;
        let mono = to_whisper_input(samples, sample_rate, channels)?;
        if mono.is_empty() {
            return Err(Error::invalid("there is no audio to transcribe"));
        }
        if is_silent(&mono) {
            return Ok(Transcript { language: self.label(resolved.language), segments: Vec::new() });
        }
        let mut guard = self.model.lock().map_err(|_| Error::internal("the speech model failed earlier; reload it"))?;
        let model: &mut SpeechModel = &mut guard;
        let job =
            TranscribeJob { filters: &self.filters, tokens: &self.tokens, special: &self.special, options: resolved };
        run_below_normal(resolved.threads, || transcribe_windows(model, &mono, &job))?
    }

    fn check_supported(&self, options: ResolvedOptions) -> Result<()> {
        if self.special.multilingual {
            return Ok(());
        }
        if options.translate || options.language.is_some_and(|index| index != 0) {
            return Err(Error::unsupported(
                "the installed speech model is English-only; install a multilingual one with scripts\\install.ps1 -SpeechModel base",
            ));
        }
        Ok(())
    }

    fn label(&self, language: Option<usize>) -> String {
        match language.and_then(language_code) {
            Some(code) => code.to_string(),
            None if !self.special.multilingual => "en".to_string(),
            None => "unknown".to_string(),
        }
    }
}

fn config_from(hparams: &Hparams) -> Result<Config> {
    if hparams.audio_state != hparams.text_state {
        return Err(Error::unsupported("speech models with different audio and text widths are not supported"));
    }
    Ok(Config {
        num_mel_bins: hparams.mels,
        max_source_positions: hparams.audio_ctx,
        d_model: hparams.audio_state,
        encoder_attention_heads: hparams.audio_head,
        encoder_layers: hparams.audio_layer,
        vocab_size: hparams.vocab,
        max_target_positions: hparams.text_ctx,
        decoder_attention_heads: hparams.text_head,
        decoder_layers: hparams.text_layer,
        suppress_tokens: Vec::new(),
    })
}

pub(crate) fn is_silent(samples: &[f32]) -> bool {
    samples.iter().all(|value| value.abs() < SILENCE_PEAK)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn digital_silence_and_hiss_are_silent_but_speech_levels_are_not() {
        assert!(is_silent(&[0.0; 100]));
        assert!(is_silent(&[0.0005, -0.0009, 0.0]));
        assert!(!is_silent(&[0.0, 0.02, 0.0]));
    }

    #[test]
    fn missing_model_names_the_install_command() {
        let error = Transcriber::load(Path::new(r"Z:\winpilot-speech-test\ggml-none.bin")).err().unwrap();
        assert!(error.message.contains("-SpeechModel"));
    }

    #[test]
    fn config_maps_the_hparams() {
        let hparams = Hparams {
            vocab: 51_865,
            audio_ctx: 1_500,
            audio_state: 512,
            audio_head: 8,
            audio_layer: 6,
            text_ctx: 448,
            text_state: 512,
            text_head: 8,
            text_layer: 6,
            mels: 80,
        };
        let config = config_from(&hparams).unwrap();
        assert_eq!((config.d_model, config.max_target_positions, config.num_mel_bins), (512, 448, 80));
        assert!(config_from(&Hparams { text_state: 384, ..hparams }).is_err());
    }

    fn installed() -> Option<Transcriber> {
        let path = crate::paths::installed_model_path().ok()?;
        Some(Transcriber::load(&path).unwrap())
    }

    #[test]
    #[ignore = "needs the speech model"]
    fn silence_has_no_segments() {
        let Some(transcriber) = installed() else { return };
        let silence = vec![0.0; 48_000 * 2 * 3];
        let transcript = transcriber.transcribe(&silence, 48_000, 2, &TranscribeOptions::default()).unwrap();
        assert!(transcript.segments.is_empty(), "{transcript:?}");
    }

    #[test]
    #[ignore = "needs the speech model"]
    fn quiet_tone_is_not_transcribed_as_words() {
        let Some(transcriber) = installed() else { return };
        let tone: Vec<f32> =
            (0..16_000 * 4).map(|index| 0.01 * (index as f32 * 2.0 * std::f32::consts::PI / 160.0).sin()).collect();
        let options = TranscribeOptions { language: Some("en".to_string()), ..TranscribeOptions::default() };
        let transcript = transcriber.transcribe(&tone, 16_000, 1, &options).unwrap();
        assert_eq!(transcript.language, "en");
        assert!(transcript.segments.len() <= 1, "{transcript:?}");
    }

    #[test]
    #[ignore = "needs the speech model and WINPILOT_SPEECH_SAMPLE"]
    fn transcribes_the_sample_named_in_the_environment() {
        let Some(sample) = std::env::var_os("WINPILOT_SPEECH_SAMPLE") else { return };
        let Some(transcriber) = installed() else { return };
        let (samples, rate, channels) = crate::media::decode_media(Path::new(&sample)).unwrap();
        let language = std::env::var("WINPILOT_SPEECH_LANGUAGE").ok();
        let options = TranscribeOptions { language, ..TranscribeOptions::default() };
        let started = std::time::Instant::now();
        let transcript = transcriber.transcribe(&samples, rate, channels, &options).unwrap();
        let elapsed = started.elapsed().as_secs_f64();
        let audio = samples.len() as f64 / f64::from(rate) / f64::from(channels);
        eprintln!("{audio:.1} s of audio in {elapsed:.1} s ({:.1}x real time)", audio / elapsed);
        eprintln!("language {}\n{:#?}", transcript.language, transcript.segments);
        assert!(!transcript.segments.is_empty());
    }
}
