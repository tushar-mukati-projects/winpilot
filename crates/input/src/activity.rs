use std::{
    sync::{Arc, Weak},
    time::{Duration, Instant},
};

use deckhand_core::Result;
use parking_lot::Mutex;

use crate::{
    input_clock::{last_user_input, last_user_press, start_clock},
    message_thread::MessageThread,
    raw_input_sink::RawInputSink,
};

static RUNNING_MONITOR: Mutex<Weak<MessageThread>> = Mutex::new(Weak::new());

#[derive(Debug)]
pub struct ActivityMonitor {
    monitor_thread: Option<Arc<MessageThread>>,
}

impl ActivityMonitor {
    pub fn spawn() -> Result<Self> {
        let mut running = RUNNING_MONITOR.lock();
        if let Some(shared) = running.upgrade() {
            return Ok(Self { monitor_thread: Some(shared) });
        }
        start_clock();
        let (thread, ()) = MessageThread::spawn("deckhand-activity", || RawInputSink::start().map(|sink| (sink, ())))?;
        let thread = Arc::new(thread);
        *running = Arc::downgrade(&thread);
        Ok(Self { monitor_thread: Some(thread) })
    }

    pub fn last_physical_input(&self) -> Option<Instant> {
        last_user_input()
    }

    pub fn last_physical_press(&self) -> Option<Instant> {
        last_user_press()
    }

    pub fn idle_for(&self) -> Option<Duration> {
        self.last_physical_input().map(|at| at.elapsed())
    }
}

impl Drop for ActivityMonitor {
    fn drop(&mut self) {
        let _running = RUNNING_MONITOR.lock();
        self.monitor_thread = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn monitor_can_be_shared_across_threads() {
        assert_send_sync::<ActivityMonitor>();
    }

    #[test]
    #[ignore = "registers for system-wide raw input"]
    fn second_spawn_shares_the_running_monitor() {
        let first = ActivityMonitor::spawn().unwrap();
        let second = ActivityMonitor::spawn().unwrap();
        let first_thread = first.monitor_thread.as_ref().unwrap();
        let second_thread = second.monitor_thread.as_ref().unwrap();
        assert!(Arc::ptr_eq(first_thread, second_thread));
        drop(first);
        drop(second);
        assert!(RUNNING_MONITOR.lock().upgrade().is_none());
    }
}
