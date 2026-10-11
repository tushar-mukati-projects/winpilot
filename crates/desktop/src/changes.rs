use std::time::{Duration, Instant};

use deckhand_core::{Result, WindowRef};

pub struct ChangeMonitor {
    _private: (),
}

impl ChangeMonitor {
    pub fn spawn() -> Result<Self> {
        todo!()
    }

    pub fn last_change(&self, pid: u32) -> Option<Instant> {
        todo!("{pid}")
    }

    pub fn wait_quiet(&self, pid: u32, quiet: Duration, max: Duration) -> bool {
        todo!("{pid} {quiet:?} {max:?}")
    }

    pub fn wait_any_change(&self, since: Instant, timeout: Duration) -> bool {
        todo!("{since:?} {timeout:?}")
    }

    pub fn windows_opened_since(&self, since: Instant) -> Vec<WindowRef> {
        todo!("{since:?}")
    }
}
