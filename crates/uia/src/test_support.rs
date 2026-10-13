use std::sync::atomic::{AtomicU32, Ordering};

use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{FindWindowW, GetForegroundWindow},
};
use windows_core::{PCWSTR, w};
use winpilot_core::{Element, Error, Rect, Result, Snapshot, WindowInfo, WindowRef, WindowState};
use winpilot_fixture::{FixtureProcess, Placement};

use crate::{UiaHandle, UiaOptions, win};

pub(crate) fn handle() -> UiaHandle {
    UiaHandle::spawn(UiaOptions::default()).unwrap()
}

pub(crate) fn first_element(snapshot: &Snapshot) -> Result<&Element> {
    snapshot.elements.first().ok_or_else(|| Error::not_found("the snapshot has no elements"))
}

pub(crate) fn window_info(window: WindowRef, class_name: &str) -> WindowInfo {
    WindowInfo {
        hwnd: window,
        title: String::new(),
        process: String::new(),
        pid: 0,
        class_name: class_name.to_string(),
        rect: Rect::default(),
        state: WindowState::Normal,
        foreground: false,
        elevated: false,
        desktop: None,
        on_current_desktop: true,
    }
}

pub(crate) fn taskbar() -> WindowInfo {
    let hwnd = unsafe { FindWindowW(w!("Shell_TrayWnd"), PCWSTR::null()) }.unwrap();
    window_info(win::window_ref(hwnd), "Shell_TrayWnd")
}

pub(crate) fn foreground() -> Option<WindowInfo> {
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return None;
    }
    Some(window_info(win::window_ref(hwnd), &class_of(hwnd)))
}

fn class_of(hwnd: HWND) -> String {
    win::class_name(win::window_ref(hwnd)).unwrap_or_default()
}

pub(crate) struct Fixture {
    _process: FixtureProcess,
    pub(crate) window: WindowInfo,
}

pub(crate) fn launch_fixture() -> Option<Fixture> {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    let Some(exe) = winpilot_fixture::fixture_exe() else {
        eprintln!("skipping: winpilot-fixture.exe is not built");
        return None;
    };
    let title = format!("WinPilot Fixture Uia {}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed));
    let process = FixtureProcess::launch(&exe, &title, Placement::OnScreen)?;
    let hwnd = process.hwnd();
    Some(Fixture { window: window_info(win::window_ref(hwnd), &class_of(hwnd)), _process: process })
}
