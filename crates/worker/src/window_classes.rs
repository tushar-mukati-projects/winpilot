use deckhand_core::WindowInfo;

const DIALOG: &str = "#32770";
pub(crate) const POPUP_MENU: &str = "#32768";
const COMBO_LIST: &str = "ComboLBox";
pub(crate) const FRAME_HOST_PROCESS: &str = "ApplicationFrameHost.exe";
const SHELL_CLASSES: [&str; 9] = [
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "NotifyIconOverflowWindow",
    "TopLevelWindowForOverflowXamlIsland",
    "XamlExplorerHostIslandWindow",
    "MultitaskingViewFrame",
    "DV2ControlHost",
];

pub(crate) fn is_shell(window: &WindowInfo) -> bool {
    SHELL_CLASSES.contains(&window.class_name.as_str())
}

pub(crate) fn is_popup_class(class_name: &str) -> bool {
    class_name == POPUP_MENU || class_name == COMBO_LIST
}

pub(crate) fn is_popup(window: &WindowInfo) -> bool {
    is_popup_class(&window.class_name)
}

pub(crate) fn is_dialog(window: &WindowInfo) -> bool {
    window.class_name == DIALOG
}

pub(crate) fn is_frame_host(window: &WindowInfo) -> bool {
    window.process.eq_ignore_ascii_case(FRAME_HOST_PROCESS)
}

#[cfg(test)]
mod tests {
    use deckhand_core::Rect;

    use super::*;
    use crate::test_fixture::window;

    fn of_class(class_name: &str) -> WindowInfo {
        WindowInfo { class_name: class_name.to_string(), ..window(1, "app.exe", "", Rect::new(0, 0, 10, 10)) }
    }

    #[test]
    fn menus_and_dropdowns_are_popups() {
        assert!(is_popup(&of_class("#32768")));
        assert!(is_popup(&of_class("ComboLBox")));
        assert!(!is_popup(&of_class("#32770")));
        assert!(!is_popup(&of_class("Notepad")));
    }

    #[test]
    fn dialogs_and_frame_hosts_are_recognised() {
        assert!(is_dialog(&of_class("#32770")));
        assert!(!is_dialog(&of_class("Notepad")));
        let frame = WindowInfo { process: "applicationframehost.exe".to_string(), ..of_class("Frame") };
        assert!(is_frame_host(&frame));
        assert!(!is_frame_host(&of_class("Frame")));
    }

    #[test]
    fn the_desktop_and_taskbars_are_shell_windows() {
        for class_name in ["Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd"] {
            assert!(is_shell(&of_class(class_name)), "{class_name}");
        }
        assert!(!is_shell(&of_class("Notepad")));
        assert!(!is_shell(&of_class("#32770")));
    }
}
