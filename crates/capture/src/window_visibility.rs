use windows::Win32::Foundation::HWND;
use winpilot_core::{Point, Rect};
use winpilot_win32::{hwnd_of, is_cloaked, is_minimized, is_visible, owner_chain, ref_of, window_at};

pub(crate) fn shows_on_screen(hwnd: HWND, visible: Rect) -> bool {
    is_visible(hwnd) && !is_cloaked(hwnd) && !is_minimized(hwnd) && is_topmost_at(hwnd, visible.center())
}

fn is_topmost_at(hwnd: HWND, point: Point) -> bool {
    window_at(point).map(hwnd_of).is_some_and(|hit| hit == hwnd || belongs_to(hwnd, hit))
}

fn belongs_to(hwnd: HWND, hit: HWND) -> bool {
    owner_chain(ref_of(hit)).contains(&ref_of(hwnd))
}

#[cfg(test)]
mod tests {
    use windows::{
        Win32::UI::WindowsAndMessaging::{CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_POPUP},
        core::{PCWSTR, w},
    };
    use winpilot_win32::{owner_of, owning_pid, top_level_windows};

    use super::*;

    struct HiddenPopup(HWND);

    impl HiddenPopup {
        fn owned_by(owner: Option<&Self>) -> Self {
            let owner = owner.map(|owner| owner.0);
            let created = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("STATIC"),
                    PCWSTR::null(),
                    WS_POPUP,
                    0,
                    0,
                    1,
                    1,
                    owner,
                    None,
                    None,
                    None,
                )
            };
            Self(created.unwrap())
        }
    }

    impl Drop for HiddenPopup {
        fn drop(&mut self) {
            let _ = unsafe { DestroyWindow(self.0) };
        }
    }

    #[test]
    #[ignore = "creates hidden windows"]
    fn a_window_owns_everything_down_its_owner_chain() {
        let dialog_owner = HiddenPopup::owned_by(None);
        let dialog = HiddenPopup::owned_by(Some(&dialog_owner));
        let dropdown = HiddenPopup::owned_by(Some(&dialog));
        assert!(belongs_to(dialog.0, dropdown.0));
        assert!(belongs_to(dialog_owner.0, dropdown.0));
        assert!(!belongs_to(dropdown.0, dialog.0));
        assert!(!belongs_to(HWND::default(), dropdown.0));
    }

    #[test]
    #[ignore = "reads the user's windows"]
    fn the_window_under_a_point_is_topmost_there() {
        crate::dpi::make_thread_per_monitor_aware().unwrap();
        let point = Point::new(10, 10);
        let Some(hit) = window_at(point).map(hwnd_of) else { return };
        assert!(is_topmost_at(hit, point));
        assert!(!is_topmost_at(HWND(std::ptr::null_mut()), point));
    }

    #[test]
    #[ignore = "reads the user's windows"]
    fn owned_windows_belong_to_their_owner_and_process_siblings_do_not() {
        let windows: Vec<HWND> = top_level_windows().unwrap();
        let window = windows.iter().find(|hwnd| owner_of(ref_of(**hwnd)).is_some()).expect("no owned window is open");
        let owner = owner_of(ref_of(*window)).map(hwnd_of).unwrap();
        assert!(belongs_to(owner, *window));
        let unowned = |hwnd: &&HWND| owner_of(ref_of(**hwnd)).is_none();
        let sibling_of = |hwnd: HWND| {
            windows.iter().filter(unowned).find(|other| **other != hwnd && owning_pid(**other) == owning_pid(hwnd))
        };
        let Some((target, sibling)) =
            windows.iter().filter(unowned).find_map(|hwnd| sibling_of(*hwnd).map(|found| (*hwnd, *found)))
        else {
            return;
        };
        assert!(!belongs_to(target, sibling));
        assert!(!belongs_to(HWND::default(), target));
    }
}
