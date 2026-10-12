use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use deckhand_capture::{AudioSource, Recorder, RecordingSpec, Redactor, Started, VideoSettings};
use deckhand_core::{
    Result,
    config::GuardConfig,
    protocol::{RecordAudio, RecordReport, RecordRequest},
    record_plan::{RecordPlan, record_plan},
};

use super::{
    output,
    redaction::DeniedWindows,
    slot::{Active, Claim, joined},
    source::{self, Redaction},
    window_watch::WindowWatch,
};
use crate::{call_scope::CallScope, engine::Engine};

const START_PATIENCE: Duration = Duration::from_secs(12);
const ABANDON_ALLOWANCE: Duration = Duration::from_secs(1);
const CLIPPED_NOTE: &str = "the region was clipped to the monitor it is on";

pub(crate) fn start(engine: &Engine, args: &RecordRequest, call: &CallScope) -> Result<RecordReport> {
    let plan = record_plan(args, &engine.config.record)?;
    engine.guard.check_running(call)?;
    let claim = engine.recording.lock().claim()?;
    let started = start_claimed(engine, args, call, plan, &claim);
    let mut slot = engine.recording.lock();
    match started {
        Ok(active) => Ok(mention_replaced(slot.begin(active), claim.replaced.as_ref())),
        Err(error) => {
            slot.abandon(claim);
            Err(error)
        }
    }
}

fn start_claimed(
    engine: &Engine,
    args: &RecordRequest,
    call: &CallScope,
    plan: RecordPlan,
    claim: &Claim,
) -> Result<Active> {
    let source = source::resolve(engine, args, call)?;
    let output = output::reserve(&engine.guard, &engine.config.record, args.path.as_deref(), plan.format)?;
    let path = output.path().display().to_string();
    let (redactor, ever_denied) = redactor_for(source.redaction, &engine.config.guard);
    let spec = RecordingSpec {
        source: source.capture,
        output,
        video: video_settings(plan),
        redactor: Some(redactor),
        kill_switch: engine.guard.kill_switch_check(),
        stop: Arc::clone(&claim.stop),
    };
    let recorder = Recorder::start(spec, patience(call))?;
    let note = start_note(recorder.started());
    Ok(Active { recorder, path, area: source.area, note, ever_denied })
}

fn start_note(started: &Started) -> Option<String> {
    let notes: Vec<String> =
        [started.clipped.then(|| CLIPPED_NOTE.to_string()), started.audio.note.clone()].into_iter().flatten().collect();
    (!notes.is_empty()).then(|| notes.join("; "))
}

fn redactor_for(redaction: Redaction, guard: &GuardConfig) -> (Redactor, Option<Arc<AtomicBool>>) {
    match redaction {
        Redaction::DeniedWindows => (Box::new(DeniedWindows::new(guard.clone())), None),
        Redaction::WholeWindowWhileDenied(window) => {
            let watch = WindowWatch::new(window, guard.clone());
            let ever_denied = watch.ever_denied();
            (Box::new(watch), Some(ever_denied))
        }
    }
}

fn patience(call: &CallScope) -> Duration {
    call.bounded(START_PATIENCE).min(call.remaining().saturating_sub(ABANDON_ALLOWANCE))
}

fn mention_replaced(mut report: RecordReport, replaced: Option<&RecordReport>) -> RecordReport {
    let Some(path) = replaced.and_then(|earlier| earlier.path.as_deref()) else { return report };
    let discarded = format!("discarded the unread report of the earlier recording; its file is saved at {path}");
    report.note = Some(joined(report.note.take(), &discarded));
    report
}

const fn video_settings(plan: RecordPlan) -> VideoSettings {
    VideoSettings {
        format: plan.format,
        fps: plan.fps,
        quality: plan.quality,
        max_seconds: plan.max_seconds,
        cursor: plan.cursor,
        max_width: plan.max_width,
        audio: audio_source(plan.audio),
    }
}

const fn audio_source(audio: RecordAudio) -> Option<AudioSource> {
    match audio {
        RecordAudio::System => Some(AudioSource::System),
        RecordAudio::Mic => Some(AudioSource::Mic),
        RecordAudio::Both => Some(AudioSource::Both),
        RecordAudio::None => None,
    }
}

#[cfg(test)]
mod tests {
    use deckhand_capture::AudioTrack;
    use deckhand_core::protocol::RecordState;

    use super::*;

    #[test]
    fn a_replaced_report_is_mentioned_in_the_start_reply() {
        let started = RecordReport { state: RecordState::Recording, ..RecordReport::idle() };
        assert_eq!(mention_replaced(started.clone(), None).note, None);
        let earlier = RecordReport {
            state: RecordState::Finished,
            path: Some("C:\\v\\a.mp4".to_string()),
            ..RecordReport::idle()
        };
        let noted = mention_replaced(started, Some(&earlier));
        assert_eq!(
            noted.note.as_deref(),
            Some("discarded the unread report of the earlier recording; its file is saved at C:\\v\\a.mp4")
        );
        let clipped = RecordReport { note: Some(CLIPPED_NOTE.to_string()), ..noted };
        let both = mention_replaced(clipped, Some(&earlier)).note.unwrap();
        assert!(both.starts_with("the region was clipped to the monitor it is on; discarded"), "{both}");
    }

    #[test]
    fn the_planned_audio_picks_the_capture_source() {
        assert_eq!(audio_source(RecordAudio::System), Some(AudioSource::System));
        assert_eq!(audio_source(RecordAudio::Mic), Some(AudioSource::Mic));
        assert_eq!(audio_source(RecordAudio::Both), Some(AudioSource::Both));
        assert_eq!(audio_source(RecordAudio::None), None);
    }

    #[test]
    fn start_notes_join_clipping_and_missing_sound() {
        let mut started = Started { width: 8, height: 8, fps: 30, clipped: false, audio: AudioTrack::default() };
        assert_eq!(start_note(&started), None);
        started.audio.note = Some("recorded without sound: no microphone is available".to_string());
        assert_eq!(start_note(&started).as_deref(), Some("recorded without sound: no microphone is available"));
        started.clipped = true;
        assert_eq!(
            start_note(&started).as_deref(),
            Some("the region was clipped to the monitor it is on; recorded without sound: no microphone is available")
        );
    }

    #[test]
    fn the_start_patience_leaves_room_to_give_up_cleanly() {
        let call = CallScope::relaxed();
        assert_eq!(patience(&call), START_PATIENCE);
    }
}
