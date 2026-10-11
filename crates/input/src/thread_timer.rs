use deckhand_core::{Error, Result};
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};

const NEW_TIMER: usize = 0;

#[derive(Debug)]
pub struct ThreadTimer {
    id: usize,
}

impl ThreadTimer {
    pub fn start(interval_ms: u32) -> Result<Self> {
        let id = unsafe { SetTimer(None, NEW_TIMER, interval_ms, None) };
        if id == 0 {
            return Err(Error::internal("could not start a thread timer"));
        }
        Ok(Self { id })
    }

    pub fn id(&self) -> usize {
        self.id
    }
}

impl Drop for ThreadTimer {
    fn drop(&mut self) {
        let _ = unsafe { KillTimer(None, self.id) };
    }
}
