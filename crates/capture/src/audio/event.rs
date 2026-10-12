use deckhand_core::{Context, Error, Result};
use windows::Win32::Foundation::{CloseHandle, HANDLE, WAIT_FAILED};
use windows::Win32::System::Threading::{CreateEventW, WaitForMultipleObjects};
use windows_core::PCWSTR;

pub(crate) struct AudioEvent(HANDLE);

impl AudioEvent {
    pub(crate) fn new() -> Result<Self> {
        let handle = unsafe { CreateEventW(None, false, false, PCWSTR::null()) }.context("creating an audio event")?;
        Ok(Self(handle))
    }

    pub(crate) const fn handle(&self) -> HANDLE {
        self.0
    }
}

impl Drop for AudioEvent {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.0) };
    }
}

pub(crate) fn wait_for_any(handles: &[HANDLE], timeout_ms: u32) -> Result<()> {
    if handles.is_empty() {
        return Ok(());
    }
    let outcome = unsafe { WaitForMultipleObjects(handles, false, timeout_ms) };
    if outcome == WAIT_FAILED {
        return Err(Error::internal(format!("waiting for audio: {}", windows_core::Error::from_thread())));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn an_unsignalled_event_times_out() {
        let event = AudioEvent::new().unwrap();
        let started = Instant::now();
        wait_for_any(&[event.handle()], 20).unwrap();
        assert!(started.elapsed() >= Duration::from_millis(15));
        wait_for_any(&[], 20).unwrap();
    }
}
