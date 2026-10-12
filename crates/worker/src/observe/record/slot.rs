use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use deckhand_capture::{AudioSource, Finished, Progress, Recorder, Started};
use deckhand_core::{
    Error, ErrorCode, Rect, Result,
    protocol::{RecordAudio, RecordReport, RecordState},
};

use super::window_watch::DENIED_NOTE;

pub(crate) struct Active {
    pub(crate) recorder: Recorder,
    pub(crate) path: String,
    pub(crate) area: Rect,
    pub(crate) note: Option<String>,
    pub(crate) ever_denied: Option<Arc<AtomicBool>>,
}

pub(crate) struct Claim {
    pub(crate) attempt: u64,
    pub(crate) stop: Arc<AtomicBool>,
    pub(crate) replaced: Option<RecordReport>,
}

#[derive(Default)]
pub(crate) struct RecordingSlot {
    state: State,
    attempts: u64,
    closed: bool,
}

#[derive(Default)]
enum State {
    #[default]
    Idle,
    Starting {
        attempt: u64,
        stop: Arc<AtomicBool>,
    },
    Recording(Active),
    Finished(RecordReport),
}

impl RecordingSlot {
    pub(crate) fn claim(&mut self) -> Result<Claim> {
        if self.closed {
            return Err(Error::new(ErrorCode::Cancelled, "Deckhand is shutting down, so no new recording can start"));
        }
        self.settle();
        match &self.state {
            State::Recording(active) => return Err(busy(&format!("already recording to {}", active.path))),
            State::Starting { .. } => return Err(busy("a recording is already starting")),
            State::Idle | State::Finished(_) => {}
        }
        self.attempts += 1;
        let stop = Arc::new(AtomicBool::new(false));
        let starting = State::Starting { attempt: self.attempts, stop: Arc::clone(&stop) };
        let replaced = match std::mem::replace(&mut self.state, starting) {
            State::Finished(report) => Some(report),
            State::Idle | State::Starting { .. } | State::Recording(_) => None,
        };
        Ok(Claim { attempt: self.attempts, stop, replaced })
    }

    pub(crate) fn begin(&mut self, active: Active) -> RecordReport {
        let report =
            recording_report(&active.path, active.recorder.started(), active.recorder.progress(), active.note.clone());
        self.state = State::Recording(active);
        report
    }

    pub(crate) fn abandon(&mut self, claim: Claim) {
        if matches!(self.state, State::Starting { attempt, .. } if attempt == claim.attempt) {
            self.state = claim.replaced.map_or(State::Idle, State::Finished);
        }
    }

    pub(crate) const fn close(&mut self) {
        self.closed = true;
    }

    pub(crate) fn cancel_start(&self) -> bool {
        let State::Starting { stop, .. } = &self.state else { return false };
        stop.store(true, Ordering::Release);
        true
    }

    pub(crate) const fn is_starting(&self) -> bool {
        matches!(self.state, State::Starting { .. })
    }

    pub(crate) fn status(&mut self) -> RecordReport {
        self.settle();
        match &self.state {
            State::Idle => RecordReport::idle(),
            State::Starting { .. } => RecordReport::starting(),
            State::Recording(active) => recording_report(
                &active.path,
                active.recorder.started(),
                active.recorder.progress(),
                active.note.clone(),
            ),
            State::Finished(report) => report.clone(),
        }
    }

    pub(crate) fn stop(&mut self) -> RecordReport {
        match std::mem::take(&mut self.state) {
            State::Idle => RecordReport::idle(),
            starting @ State::Starting { .. } => {
                self.state = starting;
                RecordReport::starting()
            }
            State::Recording(active) => finish(active),
            State::Finished(report) => report,
        }
    }

    pub(crate) const fn indicator_area(&self) -> Option<Rect> {
        match &self.state {
            State::Recording(active) => Some(active.area),
            State::Idle | State::Starting { .. } | State::Finished(_) => None,
        }
    }

    fn settle(&mut self) {
        if !matches!(&self.state, State::Recording(active) if active.recorder.has_ended()) {
            return;
        }
        if let State::Recording(active) = std::mem::take(&mut self.state) {
            self.state = State::Finished(finish(active));
        }
    }
}

fn busy(what: &str) -> Error {
    Error::new(ErrorCode::Busy, format!("{what}; stop it first with observe mode=record action=stop"))
}

fn finish(active: Active) -> RecordReport {
    let started = active.recorder.started().clone();
    let denied = active.ever_denied.as_ref().is_some_and(|flag| flag.load(Ordering::Acquire));
    let mut report = finished_report(&active.path, &started, &active.recorder.stop());
    if denied {
        report.note = Some(joined(report.note.take(), DENIED_NOTE));
    }
    report
}

pub(crate) fn joined(note: Option<String>, more: &str) -> String {
    match note {
        Some(note) => format!("{note}; {more}"),
        None => more.to_string(),
    }
}

fn recording_report(path: &str, started: &Started, progress: Progress, note: Option<String>) -> RecordReport {
    RecordReport {
        state: RecordState::Recording,
        path: Some(path.to_string()),
        seconds: progress.seconds,
        frames: progress.frames,
        bytes: 0,
        width: started.width,
        height: started.height,
        fps: started.fps,
        audio: started.audio.source.map(record_audio),
        note,
    }
}

fn finished_report(path: &str, started: &Started, finished: &Finished) -> RecordReport {
    RecordReport {
        state: RecordState::Finished,
        path: Some(path.to_string()),
        seconds: finished.seconds,
        frames: finished.frames,
        bytes: finished.bytes,
        width: started.width,
        height: started.height,
        fps: started.fps,
        audio: finished.audio.source.map(record_audio),
        note: finished.note(),
    }
}

const fn record_audio(source: AudioSource) -> RecordAudio {
    match source {
        AudioSource::System => RecordAudio::System,
        AudioSource::Mic => RecordAudio::Mic,
        AudioSource::Both => RecordAudio::Both,
    }
}

#[cfg(test)]
mod tests {
    use deckhand_capture::{AudioTrack, EndReason};

    use super::*;

    fn started() -> Started {
        Started {
            width: 1920,
            height: 1200,
            fps: 30,
            clipped: false,
            audio: AudioTrack { source: Some(AudioSource::System), note: None },
        }
    }

    fn finished_slot(report: RecordReport) -> RecordingSlot {
        RecordingSlot { state: State::Finished(report), attempts: 0, closed: false }
    }

    fn saved(path: &str) -> RecordReport {
        RecordReport { state: RecordState::Finished, path: Some(path.to_string()), ..RecordReport::idle() }
    }

    #[test]
    fn a_running_recording_reports_progress_without_bytes() {
        let progress = Progress { frames: 90, dropped: 0, seconds: 3.0 };
        let report = recording_report("C:\\v\\a.mp4", &started(), progress, Some("clipped".to_string()));
        assert_eq!(report.state, RecordState::Recording);
        assert_eq!((report.frames, report.seconds, report.bytes), (90, 3.0, 0));
        assert_eq!((report.width, report.height, report.fps), (1920, 1200, 30));
        assert_eq!(report.note.as_deref(), Some("clipped"));
        assert_eq!(report.audio, Some(RecordAudio::System));
    }

    #[test]
    fn a_finished_recording_reports_the_file_and_why_it_ended() {
        let finished = Finished {
            frames: 300,
            dropped: 0,
            seconds: 10.0,
            bytes: 4096,
            reason: EndReason::TimeLimit(10),
            write_error: None,
            finalize_error: None,
            audio: AudioTrack { source: Some(AudioSource::Both), note: Some("sound stopped at 4.0 s: x".to_string()) },
        };
        let report = finished_report("C:\\v\\a.mp4", &started(), &finished);
        assert_eq!(report.state, RecordState::Finished);
        assert_eq!(report.path.as_deref(), Some("C:\\v\\a.mp4"));
        assert_eq!((report.frames, report.bytes), (300, 4096));
        assert_eq!(report.audio, Some(RecordAudio::Both));
        assert_eq!(
            report.note.as_deref(),
            Some("stopped automatically at max_seconds (10 s); sound stopped at 4.0 s: x")
        );
    }

    #[test]
    fn an_idle_slot_is_free_and_stops_to_idle() {
        let mut slot = RecordingSlot::default();
        assert_eq!(slot.status(), RecordReport::idle());
        assert_eq!(slot.stop(), RecordReport::idle());
        assert_eq!(slot.indicator_area(), None);
        let claim = slot.claim().unwrap();
        assert_eq!(claim.replaced, None);
    }

    #[test]
    fn a_finished_report_stays_until_stop_collects_it() {
        let report = saved("C:\\a.gif");
        let mut slot = finished_slot(report.clone());
        assert_eq!(slot.status(), report);
        assert_eq!(slot.status(), report);
        assert_eq!(slot.stop(), report);
        assert_eq!(slot.stop(), RecordReport::idle());
    }

    #[test]
    fn a_starting_slot_reports_starting_and_refuses_a_second_start() {
        let mut slot = RecordingSlot::default();
        let claim = slot.claim().unwrap();
        assert!(slot.is_starting());
        assert_eq!(slot.status(), RecordReport::starting());
        assert_eq!(slot.stop(), RecordReport::starting());
        let busy = slot.claim().err().unwrap();
        assert_eq!(busy.code, ErrorCode::Busy);
        assert!(busy.message.starts_with("a recording is already starting"), "{}", busy.message);
        assert!(!claim.stop.load(Ordering::Acquire));
        assert!(slot.cancel_start());
        assert!(claim.stop.load(Ordering::Acquire));
        slot.abandon(claim);
        assert_eq!(slot.status(), RecordReport::idle());
        assert!(!slot.cancel_start());
    }

    #[test]
    fn a_new_start_takes_the_unread_report_and_a_failed_start_puts_it_back() {
        let report = saved("C:\\clips\\old.mp4");
        let mut slot = finished_slot(report.clone());
        let claim = slot.claim().unwrap();
        assert_eq!(claim.replaced.as_ref(), Some(&report));
        slot.abandon(claim);
        assert_eq!(slot.status(), report);
    }

    #[test]
    fn a_closed_slot_refuses_new_recordings_but_still_reports() {
        let report = saved("C:\\clips\\last.mp4");
        let mut slot = finished_slot(report.clone());
        slot.close();
        assert_eq!(slot.claim().err().map(|error| error.code), Some(ErrorCode::Cancelled));
        assert_eq!(slot.status(), report);
        assert_eq!(slot.stop(), report);
    }

    #[test]
    fn notes_are_joined_with_semicolons() {
        assert_eq!(joined(None, "b"), "b");
        assert_eq!(joined(Some("a".to_string()), "b"), "a; b");
    }
}
