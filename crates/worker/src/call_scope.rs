use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use deckhand_core::{Error, Point, Result, keys::Chord, protocol::MouseButton};
use parking_lot::Mutex;

use crate::cancel::Cancel;

const SLACK: Duration = Duration::from_secs(1);
const OUT_OF_TIME: &str =
    "the call ran out of time before it finished; try again with fewer steps or a shorter timeout";

pub struct CallScope {
    cancel: Cancel,
    deadline: Instant,
    acting: AtomicBool,
    overlay_shown: AtomicBool,
    held: Mutex<Vec<HeldButton>>,
    held_keys: Mutex<Vec<Chord>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HeldButton {
    pub button: MouseButton,
    pub pressed_at: Point,
}

impl CallScope {
    pub fn new(cancel: Cancel, deadline: Instant) -> Self {
        Self {
            cancel,
            deadline,
            acting: AtomicBool::new(false),
            overlay_shown: AtomicBool::new(false),
            held: Mutex::new(Vec::new()),
            held_keys: Mutex::new(Vec::new()),
        }
    }

    pub fn note_held(&self, button: MouseButton, pressed_at: Point) {
        let mut held = self.held.lock();
        if held.iter().all(|held| held.button != button) {
            held.push(HeldButton { button, pressed_at });
        }
    }

    pub fn note_released(&self, button: MouseButton) {
        self.held.lock().retain(|held| held.button != button);
    }

    pub fn take_held(&self) -> Vec<HeldButton> {
        std::mem::take(&mut *self.held.lock())
    }

    pub fn held_buttons(&self) -> Vec<MouseButton> {
        self.held.lock().iter().map(|held| held.button).collect()
    }

    pub fn note_held_keys(&self, chord: Chord) {
        let mut held = self.held_keys.lock();
        if !held.contains(&chord) {
            held.push(chord);
        }
    }

    pub fn note_released_keys(&self, chord: Chord) {
        self.held_keys.lock().retain(|held| *held != chord);
    }

    pub fn take_held_keys(&self) -> Vec<Chord> {
        std::mem::take(&mut *self.held_keys.lock())
    }

    pub fn held_keys(&self) -> Vec<Chord> {
        self.held_keys.lock().clone()
    }

    pub fn check(&self) -> Result<()> {
        self.cancel.check()?;
        if self.expired() {
            return Err(out_of_time());
        }
        Ok(())
    }

    pub fn expired(&self) -> bool {
        Instant::now() >= self.deadline
    }

    pub fn remaining(&self) -> Duration {
        self.deadline.saturating_duration_since(Instant::now())
    }

    pub fn bounded(&self, wanted: Duration) -> Duration {
        wanted.min(self.remaining())
    }

    pub fn note_acting(&self) {
        self.acting.store(true, Ordering::SeqCst);
    }

    pub fn is_acting(&self) -> bool {
        self.acting.load(Ordering::SeqCst)
    }

    pub fn note_overlay_shown(&self) {
        self.overlay_shown.store(true, Ordering::SeqCst);
    }

    pub fn showed_overlay(&self) -> bool {
        self.overlay_shown.load(Ordering::SeqCst)
    }
}

pub fn working_deadline(started: Instant, allowed: Duration) -> Instant {
    started.checked_add(allowed.saturating_sub(SLACK)).unwrap_or(started)
}

pub fn out_of_time() -> Error {
    Error::timeout(OUT_OF_TIME)
}

#[cfg(test)]
impl CallScope {
    pub fn relaxed() -> Self {
        Self::new(Cancel::default(), working_deadline(Instant::now(), Duration::from_secs(600)))
    }
}

#[cfg(test)]
mod tests {
    use deckhand_core::ErrorCode;

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
    fn held_buttons_keep_their_first_press_point_until_released_or_taken() {
        let scope = CallScope::relaxed();
        scope.note_held(MouseButton::Left, Point::new(10, 20));
        scope.note_held(MouseButton::Left, Point::new(99, 99));
        scope.note_held(MouseButton::Right, Point::new(5, 5));
        assert_eq!(scope.held_buttons(), vec![MouseButton::Left, MouseButton::Right]);
        scope.note_released(MouseButton::Right);
        let left = HeldButton { button: MouseButton::Left, pressed_at: Point::new(10, 20) };
        assert_eq!(scope.take_held(), vec![left]);
        assert!(scope.take_held().is_empty() && scope.held_buttons().is_empty());
    }

    #[test]
    fn held_keys_are_tracked_until_released_or_taken() {
        let scope = CallScope::relaxed();
        let shift = deckhand_core::keys::parse_chord("shift").unwrap();
        let ctrl_a = deckhand_core::keys::parse_chord("ctrl+a").unwrap();
        scope.note_held_keys(shift);
        scope.note_held_keys(shift);
        scope.note_held_keys(ctrl_a);
        scope.note_released_keys(ctrl_a);
        assert_eq!(scope.held_keys(), vec![shift]);
        assert_eq!(scope.take_held_keys(), vec![shift]);
        assert!(scope.take_held_keys().is_empty());
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
    }
}
