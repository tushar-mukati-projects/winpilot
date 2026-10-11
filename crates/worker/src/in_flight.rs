use std::{
    collections::HashMap,
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use parking_lot::Mutex;

use crate::cancel::Cancel;

const FINISH_POLL: Duration = Duration::from_millis(10);

#[derive(Default)]
pub struct InFlight {
    cancels: Mutex<HashMap<u64, Cancel>>,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

impl InFlight {
    pub fn register(&self, id: u64) -> Cancel {
        let cancel = Cancel::default();
        self.cancels.lock().insert(id, cancel.clone());
        cancel
    }

    pub fn finished(&self, id: u64) {
        self.cancels.lock().remove(&id);
    }

    pub fn cancel(&self, id: u64) {
        if let Some(cancel) = self.cancels.lock().get(&id) {
            cancel.cancel();
        }
    }

    pub fn track(&self, thread: JoinHandle<()>) {
        let mut threads = self.threads.lock();
        threads.retain(|running| !running.is_finished());
        threads.push(thread);
    }

    pub fn cancel_all_and_wait(&self, patience: Duration) -> usize {
        for cancel in self.cancels.lock().values() {
            cancel.cancel();
        }
        let threads = std::mem::take(&mut *self.threads.lock());
        let started = Instant::now();
        while threads.iter().any(|thread| !thread.is_finished()) && started.elapsed() < patience {
            thread::sleep(FINISH_POLL);
        }
        let (finished, unfinished): (Vec<_>, Vec<_>) = threads.into_iter().partition(JoinHandle::is_finished);
        for thread in finished {
            let _ = thread.join();
        }
        unfinished.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spawned(in_flight: &InFlight, id: u64, ignores_cancel: bool) {
        let cancel = in_flight.register(id);
        let thread = thread::spawn(move || {
            let started = Instant::now();
            while (ignores_cancel || !cancel.is_cancelled()) && started.elapsed() < Duration::from_secs(2) {
                thread::sleep(Duration::from_millis(5));
            }
        });
        in_flight.track(thread);
    }

    #[test]
    fn cancelling_everything_waits_for_calls_that_stop() {
        let in_flight = InFlight::default();
        spawned(&in_flight, 1, false);
        spawned(&in_flight, 2, false);
        let started = Instant::now();
        assert_eq!(in_flight.cancel_all_and_wait(Duration::from_secs(1)), 0);
        assert!(started.elapsed() < Duration::from_millis(900));
    }

    #[test]
    fn a_call_that_ignores_cancelling_is_left_after_the_patience_runs_out() {
        let in_flight = InFlight::default();
        spawned(&in_flight, 1, true);
        let started = Instant::now();
        assert_eq!(in_flight.cancel_all_and_wait(Duration::from_millis(50)), 1);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn a_single_call_can_be_cancelled_and_forgotten() {
        let in_flight = InFlight::default();
        let cancel = in_flight.register(9);
        in_flight.cancel(9);
        assert!(cancel.is_cancelled());
        in_flight.finished(9);
        in_flight.cancel(9);
        assert_eq!(in_flight.cancel_all_and_wait(Duration::ZERO), 0);
    }
}
