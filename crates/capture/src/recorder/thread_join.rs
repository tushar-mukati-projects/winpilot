use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(10);

pub(crate) fn join_within<T>(handle: JoinHandle<T>, grace: Duration) -> Option<T> {
    let deadline = Instant::now().checked_add(grace).unwrap_or_else(Instant::now);
    while !handle.is_finished() {
        if Instant::now() >= deadline {
            return None;
        }
        thread::sleep(POLL);
    }
    handle.join().ok()
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn a_finished_thread_is_joined_and_a_stuck_one_is_left_behind() {
        let quick = thread::spawn(|| 7);
        assert_eq!(join_within(quick, Duration::from_secs(5)), Some(7));
        let (release, wait) = mpsc::channel::<()>();
        let stuck = thread::spawn(move || wait.recv().is_ok());
        let started = Instant::now();
        assert_eq!(join_within(stuck, Duration::from_millis(50)), None);
        assert!(started.elapsed() < Duration::from_secs(2));
        drop(release);
    }
}
