use winpilot_core::{Error, Result, WindowInfo, WindowRef, WindowState};

use crate::{app_windows::app_windows, engine::Engine};

#[derive(Debug, Clone, PartialEq, Eq)]
struct Candidate {
    window: WindowInfo,
    topmost: bool,
    host: bool,
}

pub(crate) fn default_window(engine: &Engine) -> Result<WindowInfo> {
    let candidates = app_windows(engine)?
        .into_iter()
        .map(|window| Candidate {
            topmost: winpilot_desktop::is_topmost(window.hwnd),
            host: engine.host.contains(window.pid),
            window,
        })
        .collect();
    choose(candidates, winpilot_desktop::foreground_window())
        .ok_or_else(|| Error::not_found("no app window is open besides the agent's own terminal"))
}

fn choose(candidates: Vec<Candidate>, foreground: Option<WindowRef>) -> Option<WindowInfo> {
    let mut usable: Vec<Candidate> = candidates.into_iter().filter(is_usable).collect();
    let pick = usable
        .iter()
        .position(|candidate| foreground == Some(candidate.window.hwnd))
        .or_else(|| usable.iter().position(|candidate| !candidate.topmost))
        .or_else(|| (!usable.is_empty()).then_some(0))?;
    Some(usable.swap_remove(pick).window)
}

fn is_usable(candidate: &Candidate) -> bool {
    !candidate.host && candidate.window.state != WindowState::Minimized && candidate.window.on_current_desktop
}

#[cfg(test)]
mod tests {
    use winpilot_core::Rect;

    use super::*;
    use crate::test_fixture::window;

    fn candidate(hwnd: i64, topmost: bool, host: bool, state: WindowState) -> Candidate {
        let window = WindowInfo { state, ..window(hwnd, "app.exe", "App", Rect::new(0, 0, 10, 10)) };
        Candidate { window, topmost, host }
    }

    fn plain(hwnd: i64) -> Candidate {
        candidate(hwnd, false, false, WindowState::Normal)
    }

    fn chosen(candidates: Vec<Candidate>, foreground: Option<i64>) -> Option<i64> {
        choose(candidates, foreground.map(WindowRef)).map(|window| window.hwnd.0)
    }

    #[test]
    fn the_foreground_app_window_wins() {
        assert_eq!(chosen(vec![plain(1), plain(2), plain(3)], Some(2)), Some(2));
        let topmost_foreground = candidate(5, true, false, WindowState::Normal);
        assert_eq!(chosen(vec![plain(1), topmost_foreground], Some(5)), Some(5));
    }

    #[test]
    fn a_host_or_minimized_foreground_is_passed_over() {
        let host = candidate(1, false, true, WindowState::Normal);
        assert_eq!(chosen(vec![host, plain(2)], Some(1)), Some(2));
        let minimized = candidate(3, false, false, WindowState::Minimized);
        assert_eq!(chosen(vec![minimized, plain(4)], Some(3)), Some(4));
        assert_eq!(chosen(vec![plain(4)], Some(99)), Some(4));
        assert_eq!(chosen(vec![plain(4)], None), Some(4));
    }

    #[test]
    fn windows_on_other_desktops_are_never_the_default() {
        let mut elsewhere = plain(1);
        elsewhere.window.on_current_desktop = false;
        elsewhere.window.desktop = Some(2);
        assert_eq!(chosen(vec![elsewhere.clone(), plain(2)], Some(1)), Some(2));
        assert_eq!(chosen(vec![elsewhere], None), None);
    }

    #[test]
    fn topmost_windows_are_only_a_last_resort() {
        let on_top = candidate(1, true, false, WindowState::Normal);
        assert_eq!(chosen(vec![on_top.clone(), plain(2)], None), Some(2));
        assert_eq!(chosen(vec![on_top], None), Some(1));
    }

    #[test]
    fn nothing_usable_means_no_default() {
        let host = candidate(1, false, true, WindowState::Normal);
        let minimized = candidate(2, false, false, WindowState::Minimized);
        assert_eq!(chosen(vec![host, minimized], Some(1)), None);
        assert_eq!(chosen(Vec::new(), None), None);
    }
}
