use std::{path::Path, sync::Arc, time::Duration};

use deckhand_capture::{AudioSource, Pcm, capture_audio};
use deckhand_core::{
    Result,
    protocol::{HeardSegment, Hearing, ListenReport, ListenRequest, ListenSource},
};
use deckhand_speech::{Segment, TranscribeOptions, Transcriber, clock, decode_media, installed_model_path};
use parking_lot::Mutex;

use crate::{call_scope::CallScope, engine::Engine};

const MAX_FILE_SECONDS: u64 = 480;
const SILENT_PEAK: f32 = 0.000_5;
const SEGMENT_PREFIX_CHARS: usize = 9;

#[derive(Default)]
pub(crate) struct SpeechSlot {
    loaded: Mutex<Option<Arc<Transcriber>>>,
}

struct Heard {
    audio: Pcm,
    what: String,
    notes: Vec<String>,
}

pub(crate) fn run(engine: &Engine, request: &ListenRequest, call: &CallScope) -> Result<ListenReport> {
    let heard = match &request.hearing {
        Hearing::Live { source, seconds } => live(engine, *source, *seconds, call)?,
        Hearing::File(path) => file(engine, path)?,
    };
    let transcriber = engine.speech.transcriber()?;
    let options = TranscribeOptions { language: request.language.clone(), translate: request.translate, threads: None };
    let transcript =
        transcriber.transcribe(&heard.audio.samples, heard.audio.sample_rate, heard.audio.channels, &options)?;
    let (segments, truncated) = within_chars(&transcript.segments, request.max_chars);
    let mut notes = heard.notes;
    notes.extend(silence_note(&heard.audio, segments.is_empty()));
    Ok(ListenReport { heard: heard.what, language: transcript.language, segments, truncated, notes })
}

impl SpeechSlot {
    fn transcriber(&self) -> Result<Arc<Transcriber>> {
        let mut loaded = self.loaded.lock();
        if let Some(transcriber) = loaded.as_ref() {
            return Ok(Arc::clone(transcriber));
        }
        let transcriber = Arc::new(Transcriber::load(&installed_model_path()?)?);
        *loaded = Some(Arc::clone(&transcriber));
        Ok(transcriber)
    }
}

fn live(engine: &Engine, source: ListenSource, seconds: u32, call: &CallScope) -> Result<Heard> {
    let length = Duration::from_secs(u64::from(seconds));
    let check = || engine.guard.check_running(call);
    match capture_audio(capture_source(source), length, check) {
        Ok(audio) => Ok(Heard { audio, what: format!("{seconds} s of {}", source.name()), notes: Vec::new() }),
        Err(mic_problem) if source == ListenSource::Both => {
            let audio = capture_audio(AudioSource::System, length, || engine.guard.check_running(call))?;
            let note =
                format!("heard the system sound only, because the mic could not be used: {}", mic_problem.message);
            Ok(Heard { audio, what: format!("{seconds} s of {}", ListenSource::System.name()), notes: vec![note] })
        }
        Err(error) => Err(error),
    }
}

const fn capture_source(source: ListenSource) -> AudioSource {
    match source {
        ListenSource::System => AudioSource::System,
        ListenSource::Mic => AudioSource::Mic,
        ListenSource::Both => AudioSource::Both,
    }
}

fn file(engine: &Engine, path: &str) -> Result<Heard> {
    let path = engine.guard.checked_path(Path::new(path))?;
    let (samples, sample_rate, channels) = decode_media(&path)?;
    let audio = Pcm { samples, sample_rate, channels };
    let name = path.file_name().map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned());
    let total_ms = duration_ms(&audio);
    let (audio, notes) = first_minutes(audio, total_ms);
    Ok(Heard { audio, what: format!("{name} ({})", clock(total_ms)), notes })
}

fn first_minutes(mut audio: Pcm, total_ms: u64) -> (Pcm, Vec<String>) {
    let kept = usize::try_from(MAX_FILE_SECONDS * u64::from(audio.sample_rate) * u64::from(audio.channels))
        .unwrap_or(usize::MAX);
    if audio.samples.len() <= kept {
        return (audio, Vec::new());
    }
    audio.samples.truncate(kept);
    let note = format!(
        "transcribed the first {} of {}; listen handles {} of a file per call",
        clock(MAX_FILE_SECONDS * 1000),
        clock(total_ms),
        clock(MAX_FILE_SECONDS * 1000)
    );
    (audio, vec![note])
}

fn duration_ms(audio: &Pcm) -> u64 {
    let frames = audio.samples.len() as u64 / u64::from(audio.channels.max(1));
    frames.saturating_mul(1000) / u64::from(audio.sample_rate.max(1))
}

fn within_chars(segments: &[Segment], max_chars: usize) -> (Vec<HeardSegment>, bool) {
    let mut kept = Vec::new();
    let mut left = max_chars;
    for segment in segments {
        let text = segment.text.trim();
        if text.is_empty() {
            continue;
        }
        let room = left.saturating_sub(SEGMENT_PREFIX_CHARS);
        let length = text.chars().count();
        if length > room {
            let start: String = text.chars().take(room).collect();
            kept.extend((!start.trim().is_empty()).then(|| heard(segment, start.trim_end())));
            return (kept, true);
        }
        left -= SEGMENT_PREFIX_CHARS + length;
        kept.push(heard(segment, text));
    }
    (kept, false)
}

fn heard(segment: &Segment, text: &str) -> HeardSegment {
    HeardSegment { start_ms: segment.start_ms, end_ms: segment.end_ms, text: text.to_string() }
}

fn silence_note(audio: &Pcm, nothing_heard: bool) -> Option<String> {
    if !nothing_heard {
        return None;
    }
    let peak = audio.samples.iter().fold(0.0f32, |peak, sample| peak.max(sample.abs()));
    (peak < SILENT_PEAK).then(|| "the audio was silent: nothing was playing, or the source is muted".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(start_ms: u64, text: &str) -> Segment {
        Segment { start_ms, end_ms: start_ms + 1000, text: text.to_string() }
    }

    fn pcm(seconds: u32, rate: u32, channels: u16, level: f32) -> Pcm {
        Pcm { samples: vec![level; (seconds * rate) as usize * usize::from(channels)], sample_rate: rate, channels }
    }

    #[test]
    fn segments_are_trimmed_and_cut_to_the_character_budget() {
        let segments = [segment(0, " Hello there. "), segment(1000, "  "), segment(2000, "General Kenobi.")];
        let (all, cut) = within_chars(&segments, 1000);
        assert_eq!(
            all.iter().map(|segment| segment.text.as_str()).collect::<Vec<_>>(),
            ["Hello there.", "General Kenobi."]
        );
        assert!(!cut);
        let (first, cut) = within_chars(&segments, 25);
        assert_eq!(first.len(), 1);
        assert!(cut);
        let (partial, cut) = within_chars(&segments, 15);
        assert_eq!(partial.iter().map(|segment| segment.text.as_str()).collect::<Vec<_>>(), ["Hello"]);
        assert!(cut);
        let (nothing, cut) = within_chars(&segments, 5);
        assert!(nothing.is_empty() && cut);
    }

    #[test]
    fn long_files_keep_their_first_minutes_and_say_so() {
        let short = pcm(5, 100, 2, 0.1);
        let (kept, notes) = first_minutes(short.clone(), duration_ms(&short));
        assert_eq!(kept.samples.len(), short.samples.len());
        assert!(notes.is_empty());
        let long = pcm(600, 100, 2, 0.1);
        assert_eq!(duration_ms(&long), 600_000);
        let (kept, notes) = first_minutes(long, 600_000);
        assert_eq!(kept.samples.len(), 480 * 100 * 2);
        assert_eq!(notes, ["transcribed the first 08:00 of 10:00; listen handles 08:00 of a file per call"]);
    }

    #[test]
    fn silence_is_explained_only_when_nothing_was_heard() {
        assert!(silence_note(&pcm(1, 100, 1, 0.0), true).is_some());
        assert!(silence_note(&pcm(1, 100, 1, 0.3), true).is_none());
        assert!(silence_note(&pcm(1, 100, 1, 0.0), false).is_none());
    }

    #[test]
    fn listen_sources_map_onto_capture_sources() {
        assert_eq!(capture_source(ListenSource::System), AudioSource::System);
        assert_eq!(capture_source(ListenSource::Mic), AudioSource::Mic);
        assert_eq!(capture_source(ListenSource::Both), AudioSource::Both);
    }
}
