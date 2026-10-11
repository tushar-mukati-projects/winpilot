use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

use deckhand_core::WindowRef;
use parking_lot::{Condvar, Mutex};

use crate::timeouts::later;

const OPENED_WINDOW_HISTORY: usize = 64;

#[derive(Default)]
struct Activity {
    last_any: Option<Instant>,
    last_by_pid: HashMap<u32, Instant>,
    opened: VecDeque<(Instant, WindowRef)>,
}

impl Activity {
    fn record(&mut self, at: Instant, pid: Option<u32>, opened: Option<WindowRef>) {
        self.last_any = Some(at);
        if let Some(pid) = pid {
            self.last_by_pid.insert(pid, at);
        }
        if let Some(window) = opened {
            if self.opened.len() == OPENED_WINDOW_HISTORY {
                self.opened.pop_front();
            }
            self.opened.push_back((at, window));
        }
    }

    fn changed_after(&self, since: Instant) -> bool {
        self.last_any.is_some_and(|last| last > since)
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

    pub(crate) fn record(&self, at: Instant, pid: Option<u32>, opened: Option<WindowRef>) {
        self.activity.lock().record(at, pid, opened);
        self.changed.notify_all();
    }

    pub(crate) fn clear(&self) {
        *self.activity.lock() = Activity::default();
    }

    pub(crate) fn last_change(&self, pid: u32) -> Option<Instant> {
        self.activity.lock().last_by_pid.get(&pid).copied()
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

    pub(crate) fn wait_any_change(&self, since: Instant, timeout: Duration) -> bool {
        let give_up = later(Instant::now(), timeout);
        let mut activity = self.activity.lock();
        loop {
            if activity.changed_after(since) {
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

    #[test]
    fn records_per_pid_and_globally() {
        let board = ActivityBoard::new();
        let before = Instant::now();
        thread::sleep(Duration::from_millis(1));
        board.record(Instant::now(), Some(42), None);
        assert!(board.last_change(42).is_some_and(|at| at >= before));
        assert!(board.last_change(7).is_none());
        assert!(board.wait_any_change(before, Duration::ZERO));
    }

    #[test]
    fn opened_windows_keep_the_last_64_without_duplicates() {
        let board = ActivityBoard::new();
        let start = Instant::now();
        thread::sleep(Duration::from_millis(1));
        for serial in 0..100 {
            board.record(Instant::now(), None, Some(WindowRef(serial)));
        }
        board.record(Instant::now(), None, Some(WindowRef(99)));
        let opened = board.opened_since(start);
        assert_eq!(opened.len(), OPENED_WINDOW_HISTORY - 1);
        assert_eq!(opened.first(), Some(&WindowRef(37)));
        assert_eq!(opened.last(), Some(&WindowRef(99)));
    }

    #[test]
    fn opened_since_skips_older_entries() {
        let board = ActivityBoard::new();
        board.record(Instant::now(), None, Some(WindowRef(1)));
        let cutoff = Instant::now();
        thread::sleep(Duration::from_millis(2));
        board.record(Instant::now(), None, Some(WindowRef(2)));
        assert_eq!(board.opened_since(cutoff), [WindowRef(2)]);
    }

    #[test]
    fn quiet_pid_settles_after_the_quiet_period() {
        let board = ActivityBoard::new();
        board.record(Instant::now(), Some(5), None);
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
                noisy.record(Instant::now(), Some(9), None);
                thread::sleep(Duration::from_millis(5));
            }
        });
        assert!(!board.wait_quiet(9, Duration::from_millis(50), Duration::from_millis(120)));
        noise_maker.join().unwrap();
    }

    #[test]
    fn wait_any_change_wakes_on_event_and_times_out_without() {
        let board = Arc::new(ActivityBoard::new());
        let since = Instant::now();
        assert!(!board.wait_any_change(since, Duration::from_millis(20)));
        let poster = Arc::clone(&board);
        let event = thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            poster.record(Instant::now(), None, None);
        });
        let started = Instant::now();
        assert!(board.wait_any_change(since, Duration::from_secs(2)));
        assert!(started.elapsed() < Duration::from_secs(1));
        event.join().unwrap();
    }
}
