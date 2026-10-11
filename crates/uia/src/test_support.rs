use std::{
    path::PathBuf,
    process::{Child, Command},
    thread,
    time::Duration,
};

use deckhand_core::{Rect, WindowInfo, WindowRef, WindowState};
use windows::Win32::{
    Foundation::HWND,
    UI::WindowsAndMessaging::{
        FindWindowExW, FindWindowW, GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId,
    },
};
use windows_core::{PCWSTR, w};

use crate::{UiaHandle, UiaOptions, win};

pub(crate) fn handle() -> UiaHandle {
    UiaHandle::spawn(UiaOptions::default()).unwrap()
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
    let mut buffer = [0u16; 256];
    let length = unsafe { GetClassNameW(hwnd, &mut buffer) };
    String::from_utf16_lossy(&buffer[..usize::try_from(length).unwrap_or(0)])
}

pub(crate) struct Fixture {
    child: Child,
    pub(crate) window: WindowInfo,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub(crate) fn launch_fixture() -> Option<Fixture> {
    let exe = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/deckhand-fixture.exe");
    if !exe.exists() {
        eprintln!("skipping: deckhand-fixture.exe is not built");
        return None;
    }
    let mut fixture = Fixture { child: Command::new(exe).spawn().ok()?, window: window_info(WindowRef(0), "") };
    for _ in 0..50 {
        if let Some(hwnd) = fixture_window(fixture.child.id()) {
            fixture.window = window_info(win::window_ref(hwnd), &class_of(hwnd));
            return Some(fixture);
        }
        thread::sleep(Duration::from_millis(100));
    }
    eprintln!("skipping: the fixture window never appeared");
    None
}

fn fixture_window(pid: u32) -> Option<HWND> {
    let mut after = None;
    loop {
        let found = unsafe { FindWindowExW(None, after, PCWSTR::null(), w!("Deckhand Fixture")) }.ok()?;
        if found.is_invalid() {
            return None;
        }
        let mut owner = 0u32;
        unsafe { GetWindowThreadProcessId(found, Some(&raw mut owner)) };
        if owner == pid {
            return Some(found);
        }
        after = Some(found);
    }
}
