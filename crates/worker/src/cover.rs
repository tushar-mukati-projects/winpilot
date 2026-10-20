use winpilot_core::{Point, WindowInfo, WindowRef};
use winpilot_desktop::VisibleWindow;
use winpilot_win32::{hwnd_of, owner_chain, window_ex_style, window_style};

use crate::{
    engine::Engine,
    window_classes::{FRAME_HOST_PROCESS, is_captionless_popup, is_floating_class, is_popup_class, is_shell_class},
};

const MAX_POPUPS: usize = 8;
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

fn cover_facts(target: &WindowInfo, hit: WindowRef, hit_class: &str, relation: &Relation) -> CoverFacts {
    let Relation { same_program, owned_by_target, agent_terminal } = *relation;
    CoverFacts {
        same_window: target.hwnd == hit,
        same_process: same_program,
        shared_host: SHARED_HOSTS.iter().any(|host| host.eq_ignore_ascii_case(target.process.trim())),
        popup: is_popup_class(hit_class),
        owned_by_target,
        shell: is_shell_class(hit_class),
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
    let relation = relation(engine, &target, hit.hwnd, hit.pid);
    covers(cover_facts(&target, hit.hwnd, &hit.class_name, &relation))
}

pub(crate) fn popups<'a>(engine: &Engine, target: &WindowInfo, windows: &'a [VisibleWindow]) -> Vec<&'a VisibleWindow> {
    popups_above(target, windows, |hit| is_popup_of(engine, target, hit.hwnd, hit.pid, &hit.class_name))
}

pub(crate) fn is_popup_of(engine: &Engine, target: &WindowInfo, hit: WindowRef, hit_pid: u32, hit_class: &str) -> bool {
    let relation = relation(engine, target, hit, hit_pid);
    popup_of(target, hit, hit_class, &relation, || {
        let handle = hwnd_of(hit);
        is_captionless_popup(window_style(handle), window_ex_style(handle))
    })
}

fn popup_of(
    target: &WindowInfo,
    hit: WindowRef,
    hit_class: &str,
    relation: &Relation,
    captionless: impl FnOnce() -> bool,
) -> bool {
    let facts = cover_facts(target, hit, hit_class, relation);
    if facts.same_window || facts.shell || facts.agent_terminal {
        return false;
    }
    let floats = || if facts.shared_host { facts.popup } else { is_floating_class(hit_class) || captionless() };
    facts.owned_by_target || (facts.same_process && floats())
}

fn relation(engine: &Engine, target: &WindowInfo, hit: WindowRef, hit_pid: u32) -> Relation {
    Relation {
        same_program: same_program(target.pid, hit_pid, winpilot_desktop::process_image_path),
        owned_by_target: owner_chain(hit).contains(&target.hwnd),
        agent_terminal: engine.host.contains(hit_pid),
    }
}

fn popups_above<'a>(
    target: &WindowInfo,
    windows: &'a [VisibleWindow],
    is_popup: impl Fn(&VisibleWindow) -> bool,
) -> Vec<&'a VisibleWindow> {
    let Some(above) = windows.iter().position(|window| window.hwnd == target.hwnd) else { return Vec::new() };
    let mut popups: Vec<&VisibleWindow> = windows[..above].iter().filter(|hit| is_popup(hit)).collect();
    let mut area = target.rect;
    loop {
        let touching = popups.iter().filter(|popup| popup.rect.intersection(area).is_some());
        let grown = touching.fold(area, |area, popup| area.union(popup.rect));
        if grown == area {
            break;
        }
        area = grown;
    }
    popups.retain(|popup| popup.rect.intersection(area).is_some());
    popups.truncate(MAX_POPUPS);
    popups
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
    winpilot_win32::window_at(spot).is_some_and(|hit| !window_covers(engine, window.hwnd, hit))
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::WindowsAndMessaging::{WINDOW_EX_STYLE, WS_POPUP, WS_THICKFRAME};
    use winpilot_core::Rect;

    use super::*;
    use crate::test_fixture::window;

    fn app(hwnd: i64, pid: u32, process: &str, class_name: &str) -> WindowInfo {
        WindowInfo { pid, class_name: class_name.to_string(), ..window(hwnd, process, "App", Rect::new(0, 0, 10, 10)) }
    }

    fn facts(target: &WindowInfo, hit: &WindowInfo, owned_by_target: bool, agent_terminal: bool) -> CoverFacts {
        let relation = Relation { same_program: target.pid == hit.pid, owned_by_target, agent_terminal };
        cover_facts(target, hit.hwnd, &hit.class_name, &relation)
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
        assert!(!covers(cover_facts(&editor, helper.hwnd, &helper.class_name, &relation)));
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

    fn visible(hwnd: i64, pid: u32, class_name: &str, rect: Rect) -> VisibleWindow {
        let (process, title) = (String::new(), String::new());
        VisibleWindow { hwnd: WindowRef(hwnd), pid, process, title, class_name: class_name.to_string(), rect }
    }

    #[test]
    fn popups_are_floating_or_owned_windows_above_the_target_that_reach_it() {
        let target = WindowInfo { rect: Rect::new(100, 100, 800, 600), ..app(10, 1, "CapCut.exe", "Qt622QWindowIcon") };
        let menu = visible(1, 1, "#32768", Rect::new(850, 300, 200, 400));
        let submenu = visible(2, 1, "#32768", Rect::new(1040, 350, 150, 100));
        let owned_dialog = visible(3, 9, "#32770", Rect::new(300, 300, 200, 100));
        let dropdown = visible(8, 1, "Qt622QWindowToolSaveBits", Rect::new(120, 120, 50, 200));
        let windows = [
            visible(4, 2, "Chrome_WidgetWin_1", Rect::new(0, 0, 500, 500)),
            visible(5, 1, "Shell_TrayWnd", Rect::new(0, 660, 1920, 48)),
            visible(11, 1, "Qt622QWindowIcon", Rect::new(200, 200, 900, 700)),
            menu.clone(),
            submenu.clone(),
            owned_dialog.clone(),
            dropdown.clone(),
            visible(6, 1, "Qt622QWindowToolTipSaveBits", Rect::new(3000, 0, 100, 20)),
            visible(10, 1, "Qt622QWindowIcon", target.rect),
            visible(7, 1, "#32768", Rect::new(150, 150, 100, 100)),
        ];
        let is_popup = |hit: &VisibleWindow| {
            let relation = Relation {
                same_program: hit.pid == target.pid,
                owned_by_target: hit.hwnd == owned_dialog.hwnd,
                agent_terminal: false,
            };
            popup_of(&target, hit.hwnd, &hit.class_name, &relation, || hit.hwnd == dropdown.hwnd)
        };
        let popups = popups_above(&target, &windows, is_popup);
        assert_eq!(popups, [&menu, &submenu, &owned_dialog, &dropdown]);
        let area = popups.iter().fold(target.rect, |area, popup| area.union(popup.rect));
        assert_eq!(area, Rect::new(100, 100, 1090, 600));
        let elsewhere = WindowInfo { hwnd: WindowRef(99), ..target.clone() };
        assert!(popups_above(&elsewhere, &windows, is_popup).is_empty());
    }

    #[test]
    fn menus_of_another_window_in_a_shared_host_need_ownership_and_popups_are_capped() {
        let explorer = WindowInfo { rect: Rect::new(0, 0, 400, 400), ..app(20, 400, "explorer.exe", "CabinetWClass") };
        let relation = Relation { same_program: true, owned_by_target: false, agent_terminal: false };
        let thumbnail = visible(21, 400, "TaskListThumbnailWnd", Rect::new(10, 10, 50, 50));
        assert!(!popup_of(&explorer, thumbnail.hwnd, &thumbnail.class_name, &relation, || true));
        assert!(popup_of(&explorer, WindowRef(22), "#32768", &relation, || false));
        let mut windows: Vec<VisibleWindow> =
            (30..40).map(|hwnd| visible(hwnd, 400, "#32768", Rect::new(10, 10, 50, 50))).collect();
        windows.push(visible(20, 400, "CabinetWClass", explorer.rect));
        let floating = |hit: &VisibleWindow| popup_of(&explorer, hit.hwnd, &hit.class_name, &relation, || false);
        assert_eq!(popups_above(&explorer, &windows, floating).len(), MAX_POPUPS);
    }

    #[test]
    fn a_shell_tooltip_is_not_a_popup_of_a_folder_window_unless_the_folder_owns_it() {
        let explorer = app(20, 400, "explorer.exe", "CabinetWClass");
        let tooltip = WindowRef(23);
        let shell = Relation { same_program: true, owned_by_target: false, agent_terminal: false };
        assert!(!popup_of(&explorer, tooltip, "tooltips_class32", &shell, || true));
        let owned = Relation { owned_by_target: true, ..shell };
        assert!(popup_of(&explorer, tooltip, "tooltips_class32", &owned, || false));
    }

    #[test]
    fn a_frameless_qt_sibling_window_is_not_a_popup() {
        let editor = app(10, 1, "CapCut.exe", "Qt622QWindowIcon");
        let relation = Relation { same_program: true, owned_by_target: false, agent_terminal: false };
        let frameless = || is_captionless_popup(WS_POPUP | WS_THICKFRAME, WINDOW_EX_STYLE::default());
        assert!(!popup_of(&editor, WindowRef(11), "Qt622QWindowIcon", &relation, frameless));
        let dropdown = || is_captionless_popup(WS_POPUP, WINDOW_EX_STYLE::default());
        assert!(popup_of(&editor, WindowRef(12), "Qt622QWindowIcon", &relation, dropdown));
    }

    #[test]
    fn another_process_is_never_asked_for_its_style() {
        let other = Relation { same_program: false, owned_by_target: false, agent_terminal: false };
        let asked = || panic!("read the style of another process's window");
        assert!(!popup_of(&clipchamp(), WindowRef(30), "Chrome_WidgetWin_1", &other, asked));
    }
}
