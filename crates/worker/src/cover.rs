use winpilot_core::{Point, WindowInfo, WindowRef};

use crate::{
    engine::Engine,
    window_classes::{FRAME_HOST_PROCESS, is_popup, is_shell},
};

const MAX_OWNERS: usize = 8;
const SHARED_HOSTS: [&str; 2] = [FRAME_HOST_PROCESS, "explorer.exe"];

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct CoverFacts {
    same_window: bool,
    same_process: bool,
    shared_host: bool,
    popup: bool,
    owned_by_target: bool,
    shell: bool,
    agent_terminal: bool,
}

const fn covers(facts: CoverFacts) -> bool {
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

struct Relation {
    same_program: bool,
    owned_by_target: bool,
    agent_terminal: bool,
}

fn cover_facts(target: &WindowInfo, hit: &WindowInfo, relation: &Relation) -> CoverFacts {
    let Relation { same_program, owned_by_target, agent_terminal } = *relation;
    CoverFacts {
        same_window: target.hwnd == hit.hwnd,
        same_process: same_program,
        shared_host: SHARED_HOSTS.iter().any(|host| host.eq_ignore_ascii_case(target.process.trim())),
        popup: is_popup(hit),
        owned_by_target,
        shell: is_shell(hit),
        agent_terminal,
    }
}

pub(crate) fn window_covers(engine: &Engine, target: WindowRef, hit: WindowRef) -> bool {
    if target == hit {
        return false;
    }
    let (Ok(target), Ok(hit)) = (winpilot_desktop::window(target), winpilot_desktop::window(hit)) else {
        return true;
    };
    let relation = Relation {
        same_program: same_program(target.pid, hit.pid, winpilot_desktop::process_image_path),
        owned_by_target: owned_by(hit.hwnd, target.hwnd),
        agent_terminal: engine.host.contains(hit.pid),
    };
    covers(cover_facts(&target, &hit, &relation))
}

pub(crate) fn same_program(target: u32, hit: u32, image_path: impl Fn(u32) -> Option<String>) -> bool {
    if target == hit {
        return true;
    }
    match (image_path(target), image_path(hit)) {
        (Some(target), Some(hit)) => target.eq_ignore_ascii_case(&hit),
        _ => false,
    }
}

pub(crate) fn uncovered_at(engine: &Engine, window: &WindowInfo, spot: Point) -> bool {
    winpilot_desktop::window_at(spot).is_some_and(|hit| !window_covers(engine, window.hwnd, hit))
}

fn owned_by(window: WindowRef, owner: WindowRef) -> bool {
    let mut current = window;
    for _ in 0..MAX_OWNERS {
        let Some(next) = winpilot_desktop::owner_of(current) else { return false };
        if next == owner {
            return true;
        }
        current = next;
    }
    false
}

#[cfg(test)]
mod tests {
    use winpilot_core::Rect;

    use super::*;
    use crate::test_fixture::window;

    fn app(hwnd: i64, pid: u32, process: &str, class_name: &str) -> WindowInfo {
        WindowInfo { pid, class_name: class_name.to_string(), ..window(hwnd, process, "App", Rect::new(0, 0, 10, 10)) }
    }

    fn facts(target: &WindowInfo, hit: &WindowInfo, owned_by_target: bool, agent_terminal: bool) -> CoverFacts {
        let relation = Relation { same_program: target.pid == hit.pid, owned_by_target, agent_terminal };
        cover_facts(target, hit, &relation)
    }

    fn clipchamp() -> WindowInfo {
        app(1, 100, "Clipchamp.exe", "WinUIDesktopWin32WindowClass")
    }

    #[test]
    fn windows_of_the_same_program_count_as_one_app_across_processes() {
        let paths = |pid: u32| match pid {
            1 | 2 => Some("C:/Apps/CapCut/CapCut.exe".to_string()),
            3 => Some("c:/apps/capcut/capcut.EXE".to_string()),
            4 => Some("C:/Apps/Other.exe".to_string()),
            _ => None,
        };
        assert!(same_program(7, 7, |_| None));
        assert!(same_program(1, 2, paths));
        assert!(same_program(1, 3, paths));
        assert!(!same_program(1, 4, paths));
        assert!(!same_program(1, 9, paths));
        let editor = app(10, 1, "CapCut.exe", "Qt622QWindowIcon");
        let helper = app(11, 2, "CapCut.exe", "Qt622QWindowIcon");
        let relation = Relation { same_program: true, owned_by_target: false, agent_terminal: false };
        assert!(!covers(cover_facts(&editor, &helper, &relation)));
    }

    #[test]
    fn the_target_window_itself_never_covers() {
        assert!(!covers(facts(&clipchamp(), &clipchamp(), false, false)));
    }

    #[test]
    fn a_flyout_or_dialog_of_the_same_process_does_not_cover() {
        let flyout = app(2, 100, "Clipchamp.exe", "Microsoft.UI.Content.PopupWindowSiteBridge");
        assert!(!covers(facts(&clipchamp(), &flyout, false, false)));
        let dialog = app(3, 100, "Clipchamp.exe", "#32770");
        assert!(!covers(facts(&clipchamp(), &dialog, false, false)));
    }

    #[test]
    fn an_owned_window_does_not_cover_even_from_another_process() {
        let helper = app(4, 200, "Helper.exe", "HelperWindow");
        assert!(!covers(facts(&clipchamp(), &helper, true, false)));
        assert!(covers(facts(&clipchamp(), &helper, false, false)));
    }

    #[test]
    fn other_processes_cover_and_the_agent_terminal_always_does() {
        let terminal = app(5, 300, "WindowsTerminal.exe", "CASCADIA_HOSTING_WINDOW_CLASS");
        assert!(covers(facts(&clipchamp(), &terminal, false, true)));
        assert!(covers(facts(&clipchamp(), &terminal, false, false)));
        let own_terminal = app(6, 100, "Clipchamp.exe", "Console");
        assert!(covers(facts(&clipchamp(), &own_terminal, true, true)));
    }

    #[test]
    fn the_taskbar_and_desktop_always_cover() {
        let explorer = app(7, 400, "explorer.exe", "CabinetWClass");
        let taskbar = app(8, 400, "explorer.exe", "Shell_TrayWnd");
        assert!(covers(facts(&explorer, &taskbar, false, false)));
        let desktop = app(9, 100, "Clipchamp.exe", "Progman");
        assert!(covers(facts(&clipchamp(), &desktop, false, false)));
    }

    #[test]
    fn processes_that_host_many_apps_only_share_menus_and_owned_windows() {
        let calculator = app(10, 500, FRAME_HOST_PROCESS, "ApplicationFrameWindow");
        let settings = app(11, 500, FRAME_HOST_PROCESS, "ApplicationFrameWindow");
        assert!(covers(facts(&calculator, &settings, false, false)));
        let explorer = app(12, 400, "explorer.exe", "CabinetWClass");
        let other_folder = app(13, 400, "explorer.exe", "CabinetWClass");
        assert!(covers(facts(&explorer, &other_folder, false, false)));
        let menu = app(14, 400, "explorer.exe", "#32768");
        assert!(!covers(facts(&explorer, &menu, false, false)));
        assert!(!covers(facts(&explorer, &other_folder, true, false)));
    }

    #[test]
    fn missing_windows_have_no_owner() {
        assert!(!owned_by(WindowRef(0), WindowRef(1)));
    }
}
