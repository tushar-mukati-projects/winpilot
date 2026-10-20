use winpilot_core::WindowRef;
use winpilot_win32::{live_hwnd, owning_pid, title};

use crate::process::ProcessLookup;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowLabel {
    pub pid: u32,
    pub title: String,
}

pub fn window_label(window: WindowRef) -> Option<WindowLabel> {
    let hwnd = live_hwnd(window).ok()?;
    Some(WindowLabel { pid: owning_pid(hwnd), title: title(hwnd) })
}

pub fn process_name(pid: u32) -> String {
    ProcessLookup::new().facts(pid).name
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::window_list::windows;

    #[test]
    fn a_live_window_has_the_label_and_process_of_its_listing() {
        let Some(listed) = windows().unwrap().into_iter().next() else { return };
        let Some(label) = window_label(listed.hwnd) else { return };
        assert_eq!(label.pid, listed.pid);
        assert_eq!(process_name(label.pid), listed.process);
    }

    #[test]
    fn a_missing_window_has_no_label() {
        assert_eq!(window_label(WindowRef(0)), None);
    }
}
