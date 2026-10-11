use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use super::clock::seconds_of;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Started {
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub clipped: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Progress {
    pub frames: u64,
    pub dropped: u64,
    pub seconds: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EndReason {
    Stopped,
    TimeLimit(u32),
    KillSwitch,
    WindowClosed,
    CaptureFailed(String),
    EncoderFailed(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Finished {
    pub frames: u64,
    pub dropped: u64,
    pub seconds: f64,
    pub bytes: u64,
    pub reason: EndReason,
    pub finalize_error: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct Counters {
    frames: AtomicU64,
    dropped: AtomicU64,
    ticks: AtomicU64,
    finished: AtomicBool,
}

impl Counters {
    pub(crate) fn sent(&self) {
        self.frames.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn dropped(&self, count: u64) {
        self.dropped.fetch_add(count, Ordering::Relaxed);
    }

    pub(crate) fn reached(&self, ticks: u64) {
        self.ticks.store(ticks, Ordering::Relaxed);
    }

    pub(crate) fn finish(&self) {
        self.finished.store(true, Ordering::Release);
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    pub(crate) fn progress(&self, fps: u32) -> Progress {
        Progress {
            frames: self.frames.load(Ordering::Relaxed),
            dropped: self.dropped.load(Ordering::Relaxed),
            seconds: seconds_of(self.ticks.load(Ordering::Relaxed), fps),
        }
    }
}

impl Finished {
    pub(crate) fn failed_to_join() -> Self {
        Self {
            frames: 0,
            dropped: 0,
            seconds: 0.0,
            bytes: 0,
            reason: EndReason::CaptureFailed("the recording thread stopped unexpectedly".to_string()),
            finalize_error: None,
        }
    }

    pub fn note(&self) -> Option<String> {
        let parts: Vec<String> =
            [self.reason_note(), self.dropped_note(), self.finalize_error.as_deref().map(unplayable)]
                .into_iter()
                .flatten()
                .collect();
        (!parts.is_empty()).then(|| parts.join("; "))
    }

    fn reason_note(&self) -> Option<String> {
        match &self.reason {
            EndReason::Stopped => None,
            EndReason::TimeLimit(seconds) => Some(format!("stopped automatically at max_seconds ({seconds} s)")),
            EndReason::KillSwitch => Some("stopped by the kill switch".to_string()),
            EndReason::WindowClosed => Some("stopped because the recorded window closed".to_string()),
            EndReason::CaptureFailed(error) => Some(format!("stopped early because capture failed: {error}")),
            EndReason::EncoderFailed(error) => Some(format!("stopped early because encoding failed: {error}")),
        }
    }

    fn dropped_note(&self) -> Option<String> {
        (self.dropped > 0).then(|| {
            let total = self.frames.saturating_add(self.dropped);
            format!("dropped {} of {total} frames because capture or encoding fell behind", self.dropped)
        })
    }
}

fn unplayable(error: &str) -> String {
    format!("the file may be unplayable: {error}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finished(reason: EndReason, dropped: u64, finalize_error: Option<&str>) -> Finished {
        Finished {
            frames: 372,
            dropped,
            seconds: 12.4,
            bytes: 1000,
            reason,
            finalize_error: finalize_error.map(str::to_string),
        }
    }

    #[test]
    fn a_clean_stop_has_no_note() {
        assert_eq!(finished(EndReason::Stopped, 0, None).note(), None);
    }

    #[test]
    fn notes_explain_why_the_recording_ended() {
        let note = |reason| finished(reason, 0, None).note().unwrap();
        assert_eq!(note(EndReason::TimeLimit(300)), "stopped automatically at max_seconds (300 s)");
        assert_eq!(note(EndReason::KillSwitch), "stopped by the kill switch");
        assert_eq!(note(EndReason::WindowClosed), "stopped because the recorded window closed");
        assert_eq!(note(EndReason::CaptureFailed("x".to_string())), "stopped early because capture failed: x");
        assert_eq!(note(EndReason::EncoderFailed("y".to_string())), "stopped early because encoding failed: y");
    }

    #[test]
    fn dropped_frames_and_finalize_errors_are_joined() {
        let note = finished(EndReason::KillSwitch, 3, Some("disk full")).note().unwrap();
        assert_eq!(
            note,
            "stopped by the kill switch; dropped 3 of 375 frames because capture or encoding fell behind; the file may be unplayable: disk full"
        );
    }

    #[test]
    fn counters_report_progress_in_timeline_seconds() {
        let counters = Counters::default();
        counters.sent();
        counters.sent();
        counters.dropped(4);
        counters.reached(45);
        assert_eq!(counters.progress(30), Progress { frames: 2, dropped: 4, seconds: 1.5 });
        assert!(!counters.is_finished());
        counters.finish();
        assert!(counters.is_finished());
    }
}
