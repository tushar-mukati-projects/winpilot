use windows::Win32::UI::WindowsAndMessaging::{
    WINDOW_EX_STYLE, WINDOW_STYLE, WS_CAPTION, WS_EX_TOOLWINDOW, WS_POPUP, WS_THICKFRAME,
};
use winpilot_core::WindowInfo;

const DIALOG: &str = "#32770";
pub(crate) const POPUP_MENU: &str = "#32768";
const COMBO_LIST: &str = "ComboLBox";
const FLOATING_CLASSES: [&str; 3] =
    ["tooltips_class32", "Xaml_WindowedPopupClass", "Microsoft.UI.Content.PopupWindowSiteBridge"];
const QT_PREFIX: &str = "Qt";
const QT_FLOATING_KINDS: [&str; 2] = ["QWindowPopup", "QWindowToolTip"];
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
    is_shell_class(&window.class_name)
}

pub(crate) fn is_shell_class(class_name: &str) -> bool {
    SHELL_CLASSES.contains(&class_name)
}

pub(crate) fn is_popup_class(class_name: &str) -> bool {
    class_name == POPUP_MENU || class_name == COMBO_LIST
}

pub(crate) fn is_popup(window: &WindowInfo) -> bool {
    is_popup_class(&window.class_name)
}

pub(crate) fn is_floating_class(class_name: &str) -> bool {
    is_popup_class(class_name)
        || FLOATING_CLASSES.contains(&class_name)
        || (class_name.starts_with(QT_PREFIX) && QT_FLOATING_KINDS.iter().any(|kind| class_name.contains(kind)))
}

pub(crate) fn is_captionless_popup(style: WINDOW_STYLE, ex_style: WINDOW_EX_STYLE) -> bool {
    let unframed = ex_style.contains(WS_EX_TOOLWINDOW) || !style.contains(WS_THICKFRAME);
    style.contains(WS_POPUP) && !style.contains(WS_CAPTION) && unframed
}

pub(crate) fn is_dialog(window: &WindowInfo) -> bool {
    window.class_name == DIALOG
}

pub(crate) fn is_frame_host(window: &WindowInfo) -> bool {
    window.process.eq_ignore_ascii_case(FRAME_HOST_PROCESS)
}

#[cfg(test)]
mod tests {
    use winpilot_core::Rect;

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
    fn menus_tooltips_flyouts_and_qt_popups_float_over_their_app() {
        for class_name in
            ["#32768", "tooltips_class32", "Qt5152QWindowPopupDropShadowSaveBits", "Qt622QWindowToolTipSaveBits"]
        {
            assert!(is_floating_class(class_name), "{class_name}");
        }
        for class_name in ["Qt622QWindowIcon", "Chrome_WidgetWin_1", "#32770", "QWindowPopup"] {
            assert!(!is_floating_class(class_name), "{class_name}");
        }
    }

    #[test]
    fn only_unframed_or_tool_popups_without_a_caption_are_captionless_popups() {
        let plain = WINDOW_EX_STYLE::default();
        assert!(is_captionless_popup(WS_POPUP, plain));
        assert!(is_captionless_popup(WS_POPUP | WS_THICKFRAME, WS_EX_TOOLWINDOW));
        assert!(!is_captionless_popup(WS_POPUP | WS_THICKFRAME, plain));
        assert!(!is_captionless_popup(WS_POPUP | WS_CAPTION, plain));
        assert!(!is_captionless_popup(WINDOW_STYLE::default(), plain));
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
