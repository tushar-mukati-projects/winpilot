use std::time::Duration;

use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{MSG, WM_TIMER},
};

use crate::window_timer::WindowTimer;

const TIMER_ID: usize = 1;
const SHORTEST_MS: u32 = 10;

#[derive(Debug, Default)]
pub(crate) struct FrameClock {
    timer: Option<WindowTimer>,
    interval: Option<u32>,
}

impl FrameClock {
    pub(crate) fn schedule(&mut self, hwnd: HWND, wake: Option<Duration>) {
        let Some(wake) = wake else {
            self.timer = None;
            self.interval = None;
            return;
        };
        let interval = interval_ms(wake);
        if self.interval == Some(interval) && self.timer.is_some() {
            return;
        }
        self.timer = None;
        self.timer = WindowTimer::start(hwnd, TIMER_ID, interval);
        self.interval = self.timer.as_ref().map(|_| interval);
    }

    pub(crate) fn fired(&self, message: &MSG) -> bool {
        message.message == WM_TIMER
            && self.timer.as_ref().is_some_and(|timer| timer.fired(message.hwnd, message.wParam.0))
    }
}

#[cfg(test)]
impl FrameClock {
    pub(crate) fn is_running(&self) -> bool {
        self.timer.is_some()
    }
}

fn interval_ms(wake: Duration) -> u32 {
    u32::try_from(wake.as_millis()).unwrap_or(u32::MAX).max(SHORTEST_MS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intervals_have_a_floor_and_no_overflow() {
        assert_eq!(interval_ms(Duration::from_millis(15)), 15);
        assert_eq!(interval_ms(Duration::ZERO), SHORTEST_MS);
        assert_eq!(interval_ms(Duration::from_secs(u64::MAX)), u32::MAX);
    }

    #[test]
    fn nothing_to_wait_for_means_no_timer() {
        let mut clock = FrameClock::default();
        clock.schedule(HWND::default(), None);
        assert!(!clock.is_running());
        clock.schedule(HWND::default(), Some(Duration::from_millis(15)));
        assert!(!clock.is_running());
    }
}
