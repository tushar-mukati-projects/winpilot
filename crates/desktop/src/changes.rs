use std::{
    sync::{Arc, OnceLock, Weak},
    time::{Duration, Instant},
};

use parking_lot::Mutex;
use winpilot_core::{Result, WindowRef};
use winpilot_win32::MessageThread;

use crate::{activity::ActivityBoard, win_event_hooks::install_change_hooks};

static BOARD: OnceLock<ActivityBoard> = OnceLock::new();
static RUNNING: Mutex<Weak<MessageThread>> = Mutex::new(Weak::new());

pub(crate) fn board() -> &'static ActivityBoard {
    BOARD.get_or_init(ActivityBoard::new)
}

pub struct ChangeMonitor {
    _hooks: Arc<MessageThread>,
}

impl ChangeMonitor {
    pub fn spawn() -> Result<Self> {
        let mut running = RUNNING.lock();
        if let Some(existing) = running.upgrade() {
            return Ok(Self { _hooks: existing });
        }
        board().clear();
        let (thread, ()) = MessageThread::spawn("winpilot-changes", install_change_hooks)?;
        let hooks = Arc::new(thread);
        *running = Arc::downgrade(&hooks);
        Ok(Self { _hooks: hooks })
    }

    pub fn wait_quiet(&self, pid: u32, quiet: Duration, max: Duration) -> bool {
        board().wait_quiet(pid, quiet, max)
    }

    pub fn wait_change(&self, pid: Option<u32>, since: Instant, timeout: Duration) -> bool {
        board().wait_change(pid, since, timeout)
    }

    pub fn wait_any_change(&self, since: Instant, timeout: Duration) -> bool {
        self.wait_change(None, since, timeout)
    }

    pub fn windows_opened_since(&self, since: Instant) -> Vec<WindowRef> {
        board().opened_since(since)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn change_monitor_is_send_and_sync() {
        assert_send_sync::<ChangeMonitor>();
    }

    #[test]
    fn spawns_shares_waits_and_stops() {
        let monitor = ChangeMonitor::spawn().unwrap();
        let second = ChangeMonitor::spawn().unwrap();
        let started = Instant::now();
        let _changed = monitor.wait_any_change(started, Duration::from_millis(200));
        let _own = monitor.wait_change(Some(std::process::id()), started, Duration::from_millis(50));
        assert!(started.elapsed() < Duration::from_millis(1500));
        let _opened = second.windows_opened_since(started);
        assert!(!monitor.wait_quiet(1, Duration::from_secs(10), Duration::from_millis(10)));
        drop(second);
        drop(monitor);
        assert!(RUNNING.lock().upgrade().is_none());
    }
}
