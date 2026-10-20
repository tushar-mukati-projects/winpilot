mod held;

use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use parking_lot::Mutex;
use winpilot_core::{Error, Result, keys::Chord};
use winpilot_win32::Cancel;

pub(crate) use self::held::HeldButton;
use crate::{ocr_read::OcrReading, overlay_plan::Staged};

const SLACK: Duration = Duration::from_secs(1);
const CANCELLED: &str = "the request was cancelled";
const OUT_OF_TIME: &str =
    "the call ran out of time before it finished; try again with fewer steps or a shorter timeout";

pub(crate) struct CallScope {
    cancel: Cancel,
    deadline: Instant,
    acting: AtomicBool,
    background: AtomicBool,
    overlay_shown: AtomicBool,
    staged: Mutex<Option<Staged>>,
    held: Mutex<Vec<HeldButton>>,
    held_keys: Mutex<Vec<Chord>>,
    ocr: Mutex<Option<OcrReading>>,
}

impl CallScope {
    pub(crate) fn new(cancel: Cancel, deadline: Instant) -> Self {
        Self {
            cancel,
            deadline,
            acting: AtomicBool::new(false),
            background: AtomicBool::new(false),
            overlay_shown: AtomicBool::new(false),
            staged: Mutex::new(None),
            held: Mutex::new(Vec::new()),
            held_keys: Mutex::new(Vec::new()),
            ocr: Mutex::new(None),
        }
    }

    pub(crate) fn check(&self) -> Result<()> {
        self.cancel.check(CANCELLED)?;
        if self.expired() {
            return Err(out_of_time());
        }
        Ok(())
    }

    pub(crate) fn expired(&self) -> bool {
        Instant::now() >= self.deadline
    }

    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }

    pub(crate) fn deadline_within(&self, limit: Duration) -> Instant {
        Instant::now().checked_add(limit).map_or(self.deadline, |end| end.min(self.deadline))
    }

    pub(crate) fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }

    pub(crate) fn bounded(&self, wanted: Duration) -> Duration {
        wanted.min(self.remaining())
    }

    pub(crate) fn note_acting(&self) {
        self.acting.store(true, Ordering::SeqCst);
    }

    pub(crate) fn is_acting(&self) -> bool {
        self.acting.load(Ordering::SeqCst)
    }

    pub(crate) fn note_background(&self) {
        self.background.store(true, Ordering::SeqCst);
    }

    pub(crate) fn in_background(&self) -> bool {
        self.background.load(Ordering::SeqCst)
    }

    pub(crate) fn stage(&self, staged: Staged) {
        *self.staged.lock() = Some(staged);
    }

    pub(crate) fn staged(&self) -> Option<Staged> {
        self.staged.lock().clone()
    }

    pub(crate) fn note_overlay_shown(&self) {
        self.overlay_shown.store(true, Ordering::SeqCst);
    }

    pub(crate) fn showed_overlay(&self) -> bool {
        self.overlay_shown.load(Ordering::SeqCst)
    }

    pub(crate) fn remember_ocr(&self, reading: OcrReading) {
        *self.ocr.lock() = Some(reading);
    }

    pub(crate) fn recalled_ocr(&self) -> Option<OcrReading> {
        self.ocr.lock().clone()
    }

    pub(crate) fn forget_ocr(&self) {
        *self.ocr.lock() = None;
    }
}

pub(crate) fn working_deadline(started: Instant, allowed: Duration) -> Instant {
    started.checked_add(allowed.saturating_sub(SLACK)).unwrap_or(started)
}

fn out_of_time() -> Error {
    Error::timeout(OUT_OF_TIME)
}

#[cfg(test)]
impl CallScope {
    pub(crate) fn relaxed() -> Self {
        Self::new(Cancel::default(), working_deadline(Instant::now(), Duration::from_secs(600)))
    }
}

#[cfg(test)]
mod tests {
    use winpilot_core::ErrorCode;

    use super::*;

    #[test]
    fn the_working_deadline_leaves_a_second_of_slack() {
        let started = Instant::now();
        assert_eq!(working_deadline(started, Duration::from_secs(10)), started + Duration::from_secs(9));
        assert_eq!(working_deadline(started, Duration::from_millis(400)), started);
    }

    #[test]
    fn waits_are_bounded_by_the_time_left() {
        let scope = CallScope::new(Cancel::default(), Instant::now() + Duration::from_secs(2));
        assert_eq!(scope.bounded(Duration::from_millis(10)), Duration::from_millis(10));
        assert!(scope.bounded(Duration::from_secs(10)) <= Duration::from_secs(2));
        assert!(scope.check().is_ok() && !scope.expired());
    }

    #[test]
    fn a_wait_inside_the_call_ends_with_whichever_ends_first() {
        let scope = CallScope::new(Cancel::default(), Instant::now() + Duration::from_secs(2));
        assert!(scope.deadline_within(Duration::from_millis(500)) <= Instant::now() + Duration::from_millis(500));
        assert_eq!(scope.deadline_within(Duration::from_secs(10)), scope.deadline());
        assert_eq!(scope.deadline_within(Duration::MAX), scope.deadline());
    }

    #[test]
    fn a_passed_deadline_is_a_timeout_and_cancelling_wins() {
        let cancel = Cancel::default();
        let scope = CallScope::new(cancel.clone(), Instant::now());
        assert!(scope.expired());
        assert_eq!(scope.remaining(), Duration::ZERO);
        assert_eq!(scope.bounded(Duration::from_secs(5)), Duration::ZERO);
        assert_eq!(scope.check().unwrap_err().code, ErrorCode::Timeout);
        cancel.cancel();
        assert_eq!(scope.check().unwrap_err().code, ErrorCode::Cancelled);
    }

    #[test]
    fn the_flags_start_clear() {
        let scope = CallScope::relaxed();
        assert!(!scope.is_acting());
        scope.note_acting();
        assert!(scope.is_acting());
        assert!(!scope.showed_overlay());
        scope.note_overlay_shown();
        assert!(scope.showed_overlay());
        assert!(!scope.in_background());
        scope.note_background();
        assert!(scope.in_background());
    }

    #[test]
    fn the_latest_staged_window_is_kept() {
        use crate::{overlay_plan::View, test_fixture::window};
        let scope = CallScope::relaxed();
        assert_eq!(scope.staged(), None);
        let first = window(1, "a.exe", "A", winpilot_core::Rect::new(0, 0, 10, 10));
        let second = window(2, "b.exe", "B", winpilot_core::Rect::new(0, 0, 10, 10));
        scope.stage(Staged { window: first, view: View::Watched });
        scope.stage(Staged { window: second.clone(), view: View::Driven });
        assert_eq!(scope.staged(), Some(Staged { window: second, view: View::Driven }));
    }
}
