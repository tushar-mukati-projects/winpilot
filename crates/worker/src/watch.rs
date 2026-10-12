use std::time::{Duration, Instant};

use deckhand_core::WindowInfo;

use crate::{engine::Engine, window_classes::is_frame_host};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Watch {
    pids: Vec<u32>,
}

impl Watch {
    pub(crate) fn any() -> Self {
        Self::default()
    }

    pub(crate) fn window(window: &WindowInfo) -> Self {
        let content = is_frame_host(window).then(|| deckhand_desktop::uwp_content_pid(window.hwnd)).flatten();
        Self { pids: with_content(window.pid, content) }
    }

    pub(crate) fn windows(windows: &[WindowInfo]) -> Self {
        match windows {
            [only] => Self::window(only),
            _ => Self::any(),
        }
    }

    pub(crate) fn wait_for_change(&self, engine: &Engine, since: Instant, timeout: Duration) -> bool {
        match self.pids.as_slice() {
            [pid] => engine.changes.wait_change(Some(*pid), since, timeout),
            _ => engine.changes.wait_any_change(since, timeout),
        }
    }

    pub(crate) fn wait_quiet(&self, engine: &Engine, quiet: Duration, max: Duration) -> bool {
        let started = Instant::now();
        self.pids.iter().all(|pid| engine.changes.wait_quiet(*pid, quiet, max.saturating_sub(started.elapsed())))
    }
}

fn with_content(pid: u32, content: Option<u32>) -> Vec<u32> {
    match content {
        Some(content) if content != pid => vec![pid, content],
        _ => vec![pid],
    }
}

#[cfg(test)]
mod tests {
    use deckhand_core::Rect;

    use super::*;
    use crate::test_fixture::window;

    #[test]
    fn frame_hosts_add_their_content_process() {
        assert_eq!(with_content(10, Some(20)), [10, 20]);
        assert_eq!(with_content(10, Some(10)), [10]);
        assert_eq!(with_content(10, None), [10]);
    }

    #[test]
    fn one_window_is_watched_by_process_and_several_by_any_change() {
        let first = window(1, "app.exe", "A", Rect::new(0, 0, 10, 10));
        assert_eq!(Watch::window(&first).pids, [100]);
        assert_eq!(Watch::windows(std::slice::from_ref(&first)).pids, [100]);
        assert_eq!(Watch::windows(&[first.clone(), first]), Watch::any());
        assert_eq!(Watch::windows(&[]), Watch::any());
    }
}
