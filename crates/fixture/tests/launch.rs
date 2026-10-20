#![cfg(windows)]

use std::path::Path;
use std::thread::sleep;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::UI::HiDpi::{
    AreDpiAwarenessContextsEqual, DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow,
    GetWindowDpiAwarenessContext, SetThreadDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::VK_F5;
use windows::Win32::UI::WindowsAndMessaging::{
    BM_SETCHECK, CB_GETCOUNT, CB_SETCURSEL, GetClassNameW, GetClientRect, GetDlgItem, GetForegroundWindow,
    GetWindowTextW, IsWindowVisible, LB_GETCOUNT, LB_SETCURSEL, PostMessageW, SMTO_ABORTIFHUNG, SendMessageTimeoutW,
    WM_COMMAND, WM_GETFONT, WM_KEYDOWN,
};
use windows::core::Result;
use winpilot_fixture::{FixtureProcess, Placement};

const FIXTURE_EXE: &str = env!("CARGO_BIN_EXE_winpilot-fixture");
const PATIENCE: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(25);
const SEND_TIMEOUT_MS: u32 = 5000;
const BUTTON_CLICKED: u16 = 0;
const SELECTION_CHANGED: u16 = 1;
const ACCELERATOR: u16 = 1;
const CTRL_K_COMMAND: u16 = 3002;
const CLIENT_WIDTH_AT_96_DPI: i64 = 624;
const CLIENT_HEIGHT_AT_96_DPI: i64 = 644;
const ITEM_COUNT: usize = 30;
const COLOUR_COUNT: usize = 3;
const EARLIEST_APPEARANCE: Duration = Duration::from_millis(700);

const CONTRACT: [(i32, &str); 18] = [
    (2001, "Edit"),
    (2002, "Edit"),
    (2003, "Button"),
    (2004, "Static"),
    (2005, "Button"),
    (2006, "Static"),
    (2007, "ComboBox"),
    (2008, "Static"),
    (2009, "ListBox"),
    (2010, "Static"),
    (2011, "Button"),
    (2012, "Static"),
    (2013, "Button"),
    (2014, "Static"),
    (2015, "Button"),
    (2016, "Button"),
    (2017, "Static"),
    (2020, "Static"),
];

const INITIAL_TEXTS: [(i32, &str); 12] = [
    (2003, "Click me"),
    (2004, "Clicks: 0"),
    (2005, "Enable feature"),
    (2006, "Feature: off"),
    (2008, "Colour: none"),
    (2010, "Selected: none"),
    (2011, "Open file…"),
    (2012, "Opened: none"),
    (2014, "Saved: none"),
    (2016, "Appeared"),
    (2017, ""),
    (2020, "Last key: none"),
];

struct Fixture {
    process: FixtureProcess,
    window: HWND,
}

impl Fixture {
    fn launch_without_activation() -> Self {
        let title = format!("WinPilot Fixture Test {}", std::process::id());
        let Some(process) = FixtureProcess::launch(Path::new(FIXTURE_EXE), &title, Placement::OffScreen) else {
            panic!("fixture window not found")
        };
        Self { window: process.hwnd(), process }
    }

    fn control(&self, id: i32) -> HWND {
        unsafe { GetDlgItem(Some(self.window), id) }.unwrap_or_else(|_| panic!("control {id} missing"))
    }

    fn text(&self, id: i32) -> String {
        window_text(self.control(id))
    }

    fn command(&self, id: i32, notification: u16) {
        let wparam = (usize::from(notification) << 16) | id as usize;
        send(self.window, WM_COMMAND, wparam, self.control(id).0.addr() as isize);
    }

    fn wait_for_text(&self, id: i32, expected: &str) {
        let reached = wait_for(|| (self.text(id) == expected).then_some(()));
        assert!(reached.is_some(), "control {id} never showed {expected:?}, has {:?}", self.text(id));
    }

    fn close_and_wait(&self) -> u32 {
        let Some(code) = self.process.close() else { panic!("fixture did not exit after WM_CLOSE") };
        code
    }
}

fn succeeded(result: Result<()>, action: &str) {
    if let Err(error) = result {
        panic!("{action} failed: {error}");
    }
}

fn wait_for<T>(mut probe: impl FnMut() -> Option<T>) -> Option<T> {
    let deadline = Instant::now() + PATIENCE;
    loop {
        if let Some(value) = probe() {
            return Some(value);
        }
        if Instant::now() >= deadline {
            return None;
        }
        sleep(POLL);
    }
}

fn send(target: HWND, message: u32, wparam: usize, lparam: isize) -> usize {
    let mut result = 0usize;
    let sent = unsafe {
        SendMessageTimeoutW(
            target,
            message,
            WPARAM(wparam),
            LPARAM(lparam),
            SMTO_ABORTIFHUNG,
            SEND_TIMEOUT_MS,
            Some(&raw mut result),
        )
    };
    assert_ne!(sent.0, 0, "message {message:#x} was not delivered");
    result
}

fn window_text(control: HWND) -> String {
    let mut buffer = [0u16; 512];
    let length = unsafe { GetWindowTextW(control, &mut buffer) };
    String::from_utf16_lossy(&buffer[..length.max(0) as usize])
}

fn class_name(control: HWND) -> String {
    let mut buffer = [0u16; 64];
    let length = unsafe { GetClassNameW(control, &mut buffer) };
    String::from_utf16_lossy(&buffer[..length.max(0) as usize])
}

fn assert_not_foreground(fixture: &Fixture) {
    let foreground = unsafe { GetForegroundWindow() };
    assert_ne!(foreground, fixture.window, "fixture took the foreground");
}

fn assert_contract(fixture: &Fixture) {
    for (id, class) in CONTRACT {
        assert!(class_name(fixture.control(id)).eq_ignore_ascii_case(class), "control {id} is not a {class}");
    }
    for (id, text) in INITIAL_TEXTS {
        assert_eq!(fixture.text(id), text, "initial text of {id}");
    }
    assert!(unsafe { IsWindowVisible(fixture.window) }.as_bool());
    assert!(!unsafe { IsWindowVisible(fixture.control(2016)) }.as_bool(), "Appeared must start hidden");
    assert_eq!(send(fixture.control(2009), LB_GETCOUNT, 0, 0), ITEM_COUNT);
    assert_eq!(send(fixture.control(2007), CB_GETCOUNT, 0, 0), COLOUR_COUNT);
}

struct PhysicalCoordinates {
    previous: DPI_AWARENESS_CONTEXT,
}

impl PhysicalCoordinates {
    fn for_this_thread() -> Self {
        let previous = unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        assert!(!previous.0.is_null(), "could not make the test thread per-monitor aware");
        Self { previous }
    }
}

impl Drop for PhysicalCoordinates {
    fn drop(&mut self) {
        let _replaced = unsafe { SetThreadDpiAwarenessContext(self.previous) };
    }
}

fn scaled(value: i64, dpi: u32) -> i32 {
    ((value * i64::from(dpi) + 48) / 96) as i32
}

fn assert_dpi_handling(fixture: &Fixture) {
    let context = unsafe { GetWindowDpiAwarenessContext(fixture.window) };
    let per_monitor_v2 = unsafe { AreDpiAwarenessContextsEqual(context, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    assert!(per_monitor_v2.as_bool(), "fixture is not Per-Monitor-V2 aware");
    let _physical = PhysicalCoordinates::for_this_thread();
    let dpi = unsafe { GetDpiForWindow(fixture.window) };
    let mut client = RECT::default();
    succeeded(unsafe { GetClientRect(fixture.window, &raw mut client) }, "reading the client rect");
    assert_eq!(client.right - client.left, scaled(CLIENT_WIDTH_AT_96_DPI, dpi));
    assert_eq!(client.bottom - client.top, scaled(CLIENT_HEIGHT_AT_96_DPI, dpi));
    for (id, _) in CONTRACT {
        assert_ne!(send(fixture.control(id), WM_GETFONT, 0, 0), 0, "control {id} has no font");
    }
}

fn exercise_behaviour(fixture: &Fixture) {
    fixture.command(2003, BUTTON_CLICKED);
    fixture.command(2003, BUTTON_CLICKED);
    fixture.wait_for_text(2004, "Clicks: 2");
    send(fixture.control(2005), BM_SETCHECK, 1, 0);
    fixture.command(2005, BUTTON_CLICKED);
    fixture.wait_for_text(2006, "Feature: on");
    send(fixture.control(2007), CB_SETCURSEL, 1, 0);
    fixture.command(2007, SELECTION_CHANGED);
    fixture.wait_for_text(2008, "Colour: Green");
    send(fixture.control(2009), LB_SETCURSEL, ITEM_COUNT - 1, 0);
    fixture.command(2009, SELECTION_CHANGED);
    fixture.wait_for_text(2010, "Selected: Item 30");
}

fn exercise_keys(fixture: &Fixture) {
    let notes = fixture.control(2002);
    let posted = unsafe { PostMessageW(Some(notes), WM_KEYDOWN, WPARAM(usize::from(VK_F5.0)), LPARAM(1)) };
    succeeded(posted, "posting F5");
    fixture.wait_for_text(2020, "Last key: F5");
    let wparam = (usize::from(ACCELERATOR) << 16) | usize::from(CTRL_K_COMMAND);
    send(fixture.window, WM_COMMAND, wparam, 0);
    fixture.wait_for_text(2020, "Last key: Ctrl+K");
}

fn exercise_delayed_button(fixture: &Fixture) {
    let started = Instant::now();
    fixture.command(2015, BUTTON_CLICKED);
    let appeared = wait_for(|| unsafe { IsWindowVisible(fixture.control(2016)) }.as_bool().then_some(()));
    assert!(appeared.is_some(), "Appeared never showed");
    assert!(started.elapsed() >= EARLIEST_APPEARANCE, "Appeared showed too early");
    fixture.command(2016, BUTTON_CLICKED);
    fixture.wait_for_text(2017, "Appeared clicked");
}

#[test]
#[ignore = "opens a window"]
fn fixture_exposes_its_contract_without_taking_focus() {
    let fixture = Fixture::launch_without_activation();
    assert_not_foreground(&fixture);
    assert_contract(&fixture);
    assert_dpi_handling(&fixture);
    exercise_behaviour(&fixture);
    exercise_keys(&fixture);
    exercise_delayed_button(&fixture);
    assert_not_foreground(&fixture);
    assert_eq!(fixture.close_and_wait(), 0);
}
