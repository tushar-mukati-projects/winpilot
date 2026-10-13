use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::UI::WindowsAndMessaging::{IsChild, IsIconic, IsWindowVisible, WindowFromPoint};
use winpilot_core::{Point, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Presence {
    visible: bool,
    cloaked: bool,
    minimized: bool,
    topmost_at_centre: bool,
}

impl Presence {
    const fn is_uncovered(self) -> bool {
        self.visible && !self.cloaked && !self.minimized && self.topmost_at_centre
    }
}

pub(crate) fn shows_on_screen(hwnd: HWND, visible: Rect) -> bool {
    presence(hwnd, visible).is_uncovered()
}

fn presence(hwnd: HWND, visible: Rect) -> Presence {
    Presence {
        visible: unsafe { IsWindowVisible(hwnd) }.as_bool(),
        cloaked: is_cloaked(hwnd),
        minimized: unsafe { IsIconic(hwnd) }.as_bool(),
        topmost_at_centre: is_topmost_at(hwnd, visible.center()),
    }
}

fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked = 0u32;
    let size = size_of::<u32>() as u32;
    let queried = unsafe { DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, (&raw mut cloaked).cast(), size) };
    queried.is_ok() && cloaked != 0
}

fn is_topmost_at(hwnd: HWND, point: Point) -> bool {
    let hit = unsafe { WindowFromPoint(POINT { x: point.x, y: point.y }) };
    !hit.is_invalid() && (hit == hwnd || unsafe { IsChild(hwnd, hit) }.as_bool())
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::WindowsAndMessaging::{GA_ROOT, GetAncestor};

    use super::*;

    const UNCOVERED: Presence = Presence { visible: true, cloaked: false, minimized: false, topmost_at_centre: true };

    #[test]
    fn only_visible_uncloaked_restored_topmost_windows_count_as_uncovered() {
        assert!(UNCOVERED.is_uncovered());
        assert!(!Presence { visible: false, ..UNCOVERED }.is_uncovered());
        assert!(!Presence { cloaked: true, ..UNCOVERED }.is_uncovered());
        assert!(!Presence { minimized: true, ..UNCOVERED }.is_uncovered());
        assert!(!Presence { topmost_at_centre: false, ..UNCOVERED }.is_uncovered());
    }

    #[test]
    fn the_window_under_a_point_is_topmost_there() {
        crate::dpi::make_thread_per_monitor_aware().unwrap();
        let point = Point::new(10, 10);
        let hit = unsafe { WindowFromPoint(POINT { x: point.x, y: point.y }) };
        if hit.is_invalid() {
            return;
        }
        let root = unsafe { GetAncestor(hit, GA_ROOT) };
        assert!(is_topmost_at(hit, point));
        assert!(is_topmost_at(root, point));
        assert!(!is_topmost_at(HWND(std::ptr::null_mut()), point));
    }
}
