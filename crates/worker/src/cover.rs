use deckhand_core::{WindowInfo, WindowRef};

use crate::{
    engine::Engine,
    window_classes::{FRAME_HOST_PROCESS, is_popup, is_shell},
};

const MAX_OWNERS: usize = 8;
const SHARED_HOSTS: [&str; 2] = [FRAME_HOST_PROCESS, "explorer.exe"];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CoverFacts {
    pub same_window: bool,
    pub same_process: bool,
    pub shared_host: bool,
    pub popup: bool,
    pub owned_by_target: bool,
    pub shell: bool,
    pub agent_terminal: bool,
}

pub const fn covers(facts: CoverFacts) -> bool {
    if facts.same_window {
        return false;
    }
    if facts.agent_terminal || facts.shell {
        return true;
    }
    if facts.owned_by_target {
        return false;
    }
    let same_app = facts.same_process && (!facts.shared_host || facts.popup);
    !same_app
}

pub fn cover_facts(target: &WindowInfo, hit: &WindowInfo, owned_by_target: bool, agent_terminal: bool) -> CoverFacts {
    CoverFacts {
        same_window: target.hwnd == hit.hwnd,
        same_process: target.pid == hit.pid,
        shared_host: SHARED_HOSTS.iter().any(|host| host.eq_ignore_ascii_case(target.process.trim())),
        popup: is_popup(hit),
        owned_by_target,
        shell: is_shell(hit),
        agent_terminal,
    }
}

pub fn window_covers(engine: &Engine, target: WindowRef, hit: WindowRef) -> bool {
    if target == hit {
        return false;
    }
    let (Ok(target), Ok(hit)) = (deckhand_desktop::window(target), deckhand_desktop::window(hit)) else {
        return true;
    };
    let owned = owned_by(hit.hwnd, target.hwnd);
    covers(cover_facts(&target, &hit, owned, engine.host.contains(hit.pid)))
}

fn owned_by(window: WindowRef, owner: WindowRef) -> bool {
    let mut current = window;
    for _ in 0..MAX_OWNERS {
        let Some(next) = deckhand_desktop::owner_of(current) else { return false };
        if next == owner {
            return true;
        }
        current = next;
    }
    false
}

#[cfg(test)]
mod tests {
    use deckhand_core::Rect;

    use super::*;
    use crate::test_fixture::window;

    fn app(hwnd: i64, pid: u32, process: &str, class_name: &str) -> WindowInfo {
        WindowInfo { pid, class_name: class_name.to_string(), ..window(hwnd, process, "App", Rect::new(0, 0, 10, 10)) }
    }

    fn clipchamp() -> WindowInfo {
        app(1, 100, "Clipchamp.exe", "WinUIDesktopWin32WindowClass")
    }

    #[test]
    fn the_target_window_itself_never_covers() {
        assert!(!covers(cover_facts(&clipchamp(), &clipchamp(), false, false)));
    }

    #[test]
    fn a_flyout_or_dialog_of_the_same_process_does_not_cover() {
        let flyout = app(2, 100, "Clipchamp.exe", "Microsoft.UI.Content.PopupWindowSiteBridge");
        assert!(!covers(cover_facts(&clipchamp(), &flyout, false, false)));
        let dialog = app(3, 100, "Clipchamp.exe", "#32770");
        assert!(!covers(cover_facts(&clipchamp(), &dialog, false, false)));
    }

    #[test]
    fn an_owned_window_does_not_cover_even_from_another_process() {
        let helper = app(4, 200, "Helper.exe", "HelperWindow");
        assert!(!covers(cover_facts(&clipchamp(), &helper, true, false)));
        assert!(covers(cover_facts(&clipchamp(), &helper, false, false)));
    }

    #[test]
    fn other_processes_cover_and_the_agent_terminal_always_does() {
        let terminal = app(5, 300, "WindowsTerminal.exe", "CASCADIA_HOSTING_WINDOW_CLASS");
        assert!(covers(cover_facts(&clipchamp(), &terminal, false, true)));
        assert!(covers(cover_facts(&clipchamp(), &terminal, false, false)));
        let own_terminal = app(6, 100, "Clipchamp.exe", "Console");
        assert!(covers(cover_facts(&clipchamp(), &own_terminal, true, true)));
    }

    #[test]
    fn the_taskbar_and_desktop_always_cover() {
        let explorer = app(7, 400, "explorer.exe", "CabinetWClass");
        let taskbar = app(8, 400, "explorer.exe", "Shell_TrayWnd");
        assert!(covers(cover_facts(&explorer, &taskbar, false, false)));
        let desktop = app(9, 100, "Clipchamp.exe", "Progman");
        assert!(covers(cover_facts(&clipchamp(), &desktop, false, false)));
    }

    #[test]
    fn processes_that_host_many_apps_only_share_menus_and_owned_windows() {
        let calculator = app(10, 500, FRAME_HOST_PROCESS, "ApplicationFrameWindow");
        let settings = app(11, 500, FRAME_HOST_PROCESS, "ApplicationFrameWindow");
        assert!(covers(cover_facts(&calculator, &settings, false, false)));
        let explorer = app(12, 400, "explorer.exe", "CabinetWClass");
        let other_folder = app(13, 400, "explorer.exe", "CabinetWClass");
        assert!(covers(cover_facts(&explorer, &other_folder, false, false)));
        let menu = app(14, 400, "explorer.exe", "#32768");
        assert!(!covers(cover_facts(&explorer, &menu, false, false)));
        assert!(!covers(cover_facts(&explorer, &other_folder, true, false)));
    }

    #[test]
    fn missing_windows_have_no_owner() {
        assert!(!owned_by(WindowRef(0), WindowRef(1)));
    }
}
