use winpilot_core::{WindowInfo, WindowRef};

use super::{by_pattern::KeptForeground, done::both_notes, stacking::keep_behind_front};
use crate::guard::Guard;

const OPENED: &str = "it opened";

pub(crate) struct LaunchedBehind<'a> {
    kept: KeptForeground<'a>,
}

impl<'a> LaunchedBehind<'a> {
    pub(crate) fn record(guard: &'a Guard) -> Self {
        Self { kept: KeptForeground::record(guard) }
    }

    pub(crate) fn on_appearing(&self, window: &WindowInfo) -> Option<String> {
        let handed_back = self.hand_back(window);
        let _early = keep_behind_front(window, OPENED);
        handed_back
    }

    pub(crate) fn once_settled(&self, window: &WindowInfo) -> Option<String> {
        let handed_back = self.hand_back(window);
        both_notes(handed_back, keep_behind_front(window, OPENED))
    }

    fn hand_back(&self, window: &WindowInfo) -> Option<String> {
        self.kept.hand_back_from(|front| front == window.hwnd || same_process(front, window.pid))
    }
}

fn same_process(front: WindowRef, pid: u32) -> bool {
    winpilot_desktop::window(front).is_ok_and(|front| front.pid == pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_window_is_no_process_of_the_launch() {
        assert!(!same_process(WindowRef(0), 4));
    }
}
