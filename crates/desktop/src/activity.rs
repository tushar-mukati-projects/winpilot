use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

use parking_lot::{Condvar, Mutex};
use winpilot_core::WindowRef;

use crate::timeouts::later;

const OPENED_WINDOW_HISTORY: usize = 64;
const PID_MEMORY: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChangeEvent {
    pub(crate) at: Instant,
    pub(crate) pid: Option<u32>,
    pub(crate) opened: Option<WindowRef>,
}

#[derive(Default)]
struct Activity {
    last_any: Option<Instant>,
    last_by_pid: HashMap<u32, Instant>,
    opened: VecDeque<(Instant, WindowRef)>,
}

impl Activity {
    fn record(&mut self, event: ChangeEvent) {
        self.last_any = Some(self.last_any.map_or(event.at, |last| last.max(event.at)));
        if let Some(pid) = event.pid {
            self.note_pid(pid, event.at);
        }
        if let Some(window) = event.opened {
            self.note_opened(window, event.at);
        }
    }

    fn note_pid(&mut self, pid: u32, at: Instant) {
        if !self.last_by_pid.contains_key(&pid) {
            self.forget_pids_quiet_since(at);
        }
        let last = self.last_by_pid.entry(pid).or_insert(at);
        *last = (*last).max(at);
    }

    fn forget_pids_quiet_since(&mut self, now: Instant) {
        let Some(cutoff) = now.checked_sub(PID_MEMORY) else { return };
        self.last_by_pid.retain(|_, last| *last >= cutoff);
    }

    fn note_opened(&mut self, window: WindowRef, at: Instant) {
        if self.opened.len() == OPENED_WINDOW_HISTORY {
            self.opened.pop_front();
        }
        self.opened.push_back((at, window));
    }

    fn last_change(&self, pid: Option<u32>) -> Option<Instant> {
        match pid {
            Some(pid) => self.last_by_pid.get(&pid).copied(),
            None => self.last_any,
        }
    }

    fn changed_after(&self, pid: Option<u32>, since: Instant) -> bool {
        self.last_change(pid).is_some_and(|last| last > since)
    }

    fn quiet_deadline(&self, pid: u32, waiting_since: Instant, quiet: Duration) -> Instant {
        let last = self.last_by_pid.get(&pid).copied().unwrap_or(waiting_since);
        later(last.max(waiting_since), quiet)
    }

    fn opened_since(&self, since: Instant) -> Vec<WindowRef> {
        let mut windows: Vec<WindowRef> = Vec::new();
        for &(at, window) in &self.opened {
            if at > since && !windows.contains(&window) {
                windows.push(window);
            }
        }
        windows
    }
}

pub(crate) struct ActivityBoard {
    activity: Mutex<Activity>,
    changed: Condvar,
}

impl ActivityBoard {
    pub(crate) fn new() -> Self {
        Self { activity: Mutex::new(Activity::default()), changed: Condvar::new() }
    }

    pub(crate) fn record(&self, event: ChangeEvent) {
        self.activity.lock().record(event);
        self.changed.notify_all();
    }

    pub(crate) fn clear(&self) {
        *self.activity.lock() = Activity::default();
    }

    pub(crate) fn opened_since(&self, since: Instant) -> Vec<WindowRef> {
        self.activity.lock().opened_since(since)
    }

    pub(crate) fn wait_quiet(&self, pid: u32, quiet: Duration, max: Duration) -> bool {
        let started = Instant::now();
        let give_up = later(started, max);
        let mut activity = self.activity.lock();
        loop {
            let quiet_at = activity.quiet_deadline(pid, started, quiet);
            let now = Instant::now();
            if now >= quiet_at {
                return true;
            }
            if now >= give_up {
                return false;
            }
            self.changed.wait_until(&mut activity, quiet_at.min(give_up));
        }
    }

    pub(crate) fn wait_change(&self, pid: Option<u32>, since: Instant, timeout: Duration) -> bool {
        let give_up = later(Instant::now(), timeout);
        let mut activity = self.activity.lock();
        loop {
            if activity.changed_after(pid, since) {
                return true;
            }
            if Instant::now() >= give_up {
                return false;
            }
            self.changed.wait_until(&mut activity, give_up);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, thread};

    use super::*;

    fn event(at: Instant, pid: Option<u32>, opened: Option<WindowRef>) -> ChangeEvent {
        ChangeEvent { at, pid, opened }
    }

    #[test]
    fn records_per_pid_and_globally() {
        let board = ActivityBoard::new();
        let before = Instant::now();
        thread::sleep(Duration::from_millis(1));
        board.record(event(Instant::now(), Some(42), None));
        assert!(board.wait_change(None, before, Duration::ZERO));
        assert!(board.wait_change(Some(42), before, Duration::ZERO));
        assert!(!board.wait_change(Some(7), before, Duration::ZERO));
    }

    #[test]
    fn an_event_dated_before_since_is_not_a_change() {
        let board = ActivityBoard::new();
        let since = Instant::now();
        board.record(event(since.checked_sub(Duration::from_millis(50)).unwrap(), Some(3), None));
        assert!(!board.wait_change(None, since, Duration::ZERO));
        assert!(!board.wait_change(Some(3), since, Duration::ZERO));
    }

    #[test]
    fn late_delivery_of_an_older_event_never_moves_time_backwards() {
        let mut activity = Activity::default();
        let newer = Instant::now();
        activity.record(event(newer, Some(1), None));
        activity.record(event(newer.checked_sub(Duration::from_millis(30)).unwrap(), Some(1), None));
        assert_eq!(activity.last_change(None), Some(newer));
        assert_eq!(activity.last_change(Some(1)), Some(newer));
    }

    #[test]
    fn a_new_pid_prunes_pids_quiet_for_a_minute() {
        let mut activity = Activity::default();
        let start = Instant::now();
        activity.record(event(start, Some(1), None));
        activity.record(event(start + Duration::from_secs(30), Some(2), None));
        activity.record(event(start + Duration::from_secs(61), Some(3), None));
        assert_eq!(activity.last_change(Some(1)), None);
        assert!(activity.last_change(Some(2)).is_some());
        assert!(activity.last_change(Some(3)).is_some());
        assert_eq!(activity.last_by_pid.len(), 2);
    }

    #[test]
    fn a_known_pid_does_not_trigger_pruning() {
        let mut activity = Activity::default();
        let start = Instant::now();
        activity.record(event(start, Some(1), None));
        activity.record(event(start, Some(2), None));
        activity.record(event(start + Duration::from_secs(90), Some(2), None));
        assert_eq!(activity.last_by_pid.len(), 2);
    }

    #[test]
    fn opened_windows_keep_the_last_64_without_duplicates() {
        let board = ActivityBoard::new();
        let start = Instant::now();
        thread::sleep(Duration::from_millis(1));
        for serial in 0..100 {
            board.record(event(Instant::now(), None, Some(WindowRef(serial))));
        }
        board.record(event(Instant::now(), None, Some(WindowRef(99))));
        let opened = board.opened_since(start);
        assert_eq!(opened.len(), OPENED_WINDOW_HISTORY - 1);
        assert_eq!(opened.first(), Some(&WindowRef(37)));
        assert_eq!(opened.last(), Some(&WindowRef(99)));
    }

    #[test]
    fn opened_since_skips_older_entries() {
        let board = ActivityBoard::new();
        board.record(event(Instant::now(), None, Some(WindowRef(1))));
        let cutoff = Instant::now();
        thread::sleep(Duration::from_millis(2));
        board.record(event(Instant::now(), None, Some(WindowRef(2))));
        assert_eq!(board.opened_since(cutoff), [WindowRef(2)]);
    }

    #[test]
    fn quiet_pid_settles_after_the_quiet_period() {
        let board = ActivityBoard::new();
        board.record(event(Instant::now(), Some(5), None));
        let started = Instant::now();
        assert!(board.wait_quiet(5, Duration::from_millis(30), Duration::from_secs(1)));
        assert!(started.elapsed() >= Duration::from_millis(30));
    }

    #[test]
    fn busy_pid_never_settles() {
        let board = Arc::new(ActivityBoard::new());
        let noisy = Arc::clone(&board);
        let noise_maker = thread::spawn(move || {
            for _ in 0..40 {
                noisy.record(event(Instant::now(), Some(9), None));
                thread::sleep(Duration::from_millis(5));
            }
        });
        assert!(!board.wait_quiet(9, Duration::from_millis(50), Duration::from_millis(120)));
        noise_maker.join().unwrap();
    }

    #[test]
    fn wait_change_wakes_on_event_and_times_out_without() {
        let board = Arc::new(ActivityBoard::new());
        let since = Instant::now();
        assert!(!board.wait_change(None, since, Duration::from_millis(20)));
        let poster = Arc::clone(&board);
        let event_thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            poster.record(event(Instant::now(), Some(11), None));
        });
        let started = Instant::now();
        assert!(board.wait_change(Some(11), since, Duration::from_secs(2)));
        assert!(started.elapsed() < Duration::from_secs(1));
        event_thread.join().unwrap();
    }

    #[test]
    fn wait_for_one_pid_ignores_other_pids() {
        let board = ActivityBoard::new();
        let since = Instant::now();
        thread::sleep(Duration::from_millis(1));
        board.record(event(Instant::now(), Some(12), None));
        assert!(!board.wait_change(Some(13), since, Duration::from_millis(15)));
        assert!(board.wait_change(None, since, Duration::ZERO));
    }
}
