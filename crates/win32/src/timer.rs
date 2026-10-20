use std::time::Duration;

use windows::Win32::{
    Foundation::HWND,
    System::Threading::INFINITE,
    UI::WindowsAndMessaging::{KillTimer, SetTimer},
};
use winpilot_core::{Error, Result};

const NEW_THREAD_TIMER: usize = 0;

#[derive(Debug)]
pub struct Timer {
    window: Option<HWND>,
    id: usize,
}

impl Timer {
    pub fn on_thread(interval_ms: u32) -> Result<Self> {
        let id = unsafe { SetTimer(None, NEW_THREAD_TIMER, interval_ms, None) };
        if id == 0 {
            return Err(Error::internal("could not start a thread timer"));
        }
        Ok(Self { window: None, id })
    }

    pub fn on_window(hwnd: HWND, id: usize, interval_ms: u32) -> Option<Self> {
        if hwnd.is_invalid() {
            return None;
        }
        let started = unsafe { SetTimer(Some(hwnd), id, interval_ms, None) };
        (started != 0).then_some(Self { window: Some(hwnd), id })
    }

    pub fn fired(&self, hwnd: HWND, id: usize) -> bool {
        self.id == id && self.window.map_or(hwnd.is_invalid(), |window| window == hwnd)
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        let _ = unsafe { KillTimer(self.window, self.id) };
    }
}

pub fn finite_millis(duration: Duration) -> u32 {
    u32::try_from(duration.as_millis()).unwrap_or(u32::MAX).min(INFINITE - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_timer_needs_a_live_window() {
        assert!(Timer::on_window(HWND::default(), 1, 10).is_none());
    }

    #[test]
    fn a_thread_timer_fires_by_its_id_without_a_window() {
        let timer = Timer::on_thread(60_000).unwrap();
        assert!(timer.fired(HWND::default(), timer.id));
        assert!(!timer.fired(HWND::default(), timer.id + 1));
        assert!(!timer.fired(HWND(0x10 as *mut _), timer.id));
    }

    #[test]
    fn millis_never_reach_infinite() {
        assert_eq!(finite_millis(Duration::from_millis(250)), 250);
        assert_eq!(finite_millis(Duration::MAX), INFINITE - 1);
    }
}
