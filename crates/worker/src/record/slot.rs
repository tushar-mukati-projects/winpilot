use deckhand_capture::{Finished, Progress, Recorder, Started};
use deckhand_core::{
    Error, ErrorCode, Rect, Result,
    protocol::{RecordReport, RecordState},
};

pub struct Active {
    pub recorder: Recorder,
    pub path: String,
    pub area: Rect,
    pub note: Option<String>,
}

#[derive(Default)]
pub struct RecordingSlot {
    state: State,
}

#[derive(Default)]
enum State {
    #[default]
    Idle,
    Recording(Active),
    Finished(RecordReport),
}

impl RecordingSlot {
    pub fn ensure_free(&mut self) -> Result<()> {
        self.settle();
        let State::Recording(active) = &self.state else { return Ok(()) };
        Err(Error::new(
            ErrorCode::Busy,
            format!("already recording to {}; stop it first with record action stop", active.path),
        ))
    }

    pub fn begin(&mut self, active: Active) -> RecordReport {
        let report =
            recording_report(&active.path, active.recorder.started(), active.recorder.progress(), active.note.clone());
        self.state = State::Recording(active);
        report
    }

    pub fn status(&mut self) -> RecordReport {
        self.settle();
        match &self.state {
            State::Idle => RecordReport::idle(),
            State::Recording(active) => recording_report(
                &active.path,
                active.recorder.started(),
                active.recorder.progress(),
                active.note.clone(),
            ),
            State::Finished(report) => report.clone(),
        }
    }

    pub fn stop(&mut self) -> RecordReport {
        match std::mem::take(&mut self.state) {
            State::Idle => RecordReport::idle(),
            State::Recording(active) => finish(active),
            State::Finished(report) => report,
        }
    }

    pub const fn indicator_area(&self) -> Option<Rect> {
        match &self.state {
            State::Recording(active) => Some(active.area),
            State::Idle | State::Finished(_) => None,
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

fn finish(active: Active) -> RecordReport {
    let started = active.recorder.started();
    finished_report(&active.path, started, &active.recorder.stop())
}

pub fn recording_report(path: &str, started: Started, progress: Progress, note: Option<String>) -> RecordReport {
    RecordReport {
        state: RecordState::Recording,
        path: Some(path.to_string()),
        seconds: progress.seconds,
        frames: progress.frames,
        bytes: 0,
        width: started.width,
        height: started.height,
        fps: started.fps,
        note,
    }
}

pub fn finished_report(path: &str, started: Started, finished: &Finished) -> RecordReport {
    RecordReport {
        state: RecordState::Finished,
        path: Some(path.to_string()),
        seconds: finished.seconds,
        frames: finished.frames,
        bytes: finished.bytes,
        width: started.width,
        height: started.height,
        fps: started.fps,
        note: finished.note(),
    }
}

#[cfg(test)]
mod tests {
    use deckhand_capture::EndReason;

    use super::*;

    const STARTED: Started = Started { width: 1920, height: 1200, fps: 30, clipped: false };

    #[test]
    fn a_running_recording_reports_progress_without_bytes() {
        let progress = Progress { frames: 90, dropped: 0, seconds: 3.0 };
        let report = recording_report("C:\\v\\a.mp4", STARTED, progress, Some("clipped".to_string()));
        assert_eq!(report.state, RecordState::Recording);
        assert_eq!((report.frames, report.seconds, report.bytes), (90, 3.0, 0));
        assert_eq!((report.width, report.height, report.fps), (1920, 1200, 30));
        assert_eq!(report.note.as_deref(), Some("clipped"));
    }

    #[test]
    fn a_finished_recording_reports_the_file_and_why_it_ended() {
        let finished = Finished {
            frames: 300,
            dropped: 0,
            seconds: 10.0,
            bytes: 4096,
            reason: EndReason::TimeLimit(10),
            finalize_error: None,
        };
        let report = finished_report("C:\\v\\a.mp4", STARTED, &finished);
        assert_eq!(report.state, RecordState::Finished);
        assert_eq!(report.path.as_deref(), Some("C:\\v\\a.mp4"));
        assert_eq!((report.frames, report.bytes), (300, 4096));
        assert_eq!(report.note.as_deref(), Some("stopped automatically at max_seconds (10 s)"));
    }

    #[test]
    fn an_idle_slot_is_free_and_stops_to_idle() {
        let mut slot = RecordingSlot::default();
        assert!(slot.ensure_free().is_ok());
        assert_eq!(slot.status(), RecordReport::idle());
        assert_eq!(slot.stop(), RecordReport::idle());
        assert_eq!(slot.indicator_area(), None);
    }

    #[test]
    fn a_finished_report_stays_until_stop_collects_it() {
        let report =
            RecordReport { state: RecordState::Finished, path: Some("C:\\a.gif".to_string()), ..RecordReport::idle() };
        let mut slot = RecordingSlot { state: State::Finished(report.clone()) };
        assert!(slot.ensure_free().is_ok());
        assert_eq!(slot.status(), report);
        assert_eq!(slot.status(), report);
        assert_eq!(slot.stop(), report);
        assert_eq!(slot.stop(), RecordReport::idle());
    }
}
