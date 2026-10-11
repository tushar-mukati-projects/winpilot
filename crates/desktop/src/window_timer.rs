use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{KillTimer, SetTimer},
};

#[derive(Debug)]
pub(crate) struct WindowTimer {
    hwnd: HWND,
    id: usize,
}

impl WindowTimer {
    pub(crate) fn start(hwnd: HWND, id: usize, interval_ms: u32) -> Option<Self> {
        if hwnd.is_invalid() {
            return None;
        }
        let started = unsafe { SetTimer(Some(hwnd), id, interval_ms, None) };
        (started != 0).then_some(Self { hwnd, id })
    }

    pub(crate) fn fired(&self, hwnd: HWND, id: usize) -> bool {
        self.hwnd == hwnd && self.id == id
    }
}

impl Drop for WindowTimer {
    fn drop(&mut self) {
        let _ = unsafe { KillTimer(Some(self.hwnd), self.id) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_timer_needs_a_live_window() {
        assert!(WindowTimer::start(HWND::default(), 1, 10).is_none());
    }
}
