use std::{
    path::PathBuf,
    sync::{
        OnceLock,
        atomic::{AtomicU32, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    UI::WindowsAndMessaging::{GetDlgItem, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_GETTEXT},
};
use winpilot_core::{
    Config, Element, ElementRef, ElementState, Rect, Role, Snapshot, WindowInfo, WindowRef, WindowState,
};
use winpilot_fixture::{FixtureProcess, Placement};

use crate::engine::Engine;

const PATIENCE: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(25);
const QUIET: Duration = Duration::from_millis(200);
const SEND_TIMEOUT_MS: u32 = 2000;

pub(crate) fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let _ = winpilot_desktop::enable_dpi_awareness();
        let mut config = Config::default();
        config.overlay.enabled = false;
        config.guard.kill_hotkey = String::new();
        config.guard.yield_ms = 0;
        Engine::start(config, 0, "winpilot-test").unwrap()
    })
}

pub(crate) fn window(hwnd: i64, process: &str, title: &str, rect: Rect) -> WindowInfo {
    WindowInfo {
        hwnd: WindowRef(hwnd),
        title: title.to_string(),
        process: process.to_string(),
        pid: 100,
        class_name: "#32770".to_string(),
        rect,
        state: WindowState::Normal,
        foreground: false,
        elevated: false,
        desktop: None,
        on_current_desktop: true,
    }
}

pub(crate) fn element(id: u32, depth: u16, role: Role, name: &str) -> Element {
    Element {
        id: ElementRef(id),
        depth,
        role,
        name: name.to_string(),
        value: None,
        automation_id: String::new(),
        class_name: String::new(),
        rect: Rect::new(10, 10, 50, 20),
        state: ElementState { enabled: true, ..ElementState::default() },
        patterns: winpilot_core::Patterns::default(),
    }
}

pub(crate) fn with_id(element: Element, automation_id: &str) -> Element {
    Element { automation_id: automation_id.to_string(), ..element }
}

pub(crate) fn snapshot(title: &str, elements: Vec<Element>) -> Snapshot {
    let window = window(0x10, "app.exe", title, Rect::new(0, 0, 800, 600));
    Snapshot { window, elements, focused: None, truncated: false }
}

pub(crate) fn fixture_exe() -> Option<PathBuf> {
    let found = winpilot_fixture::fixture_exe();
    if found.is_none() {
        eprintln!("skipping: winpilot-fixture.exe is not built (cargo build -p winpilot-fixture)");
    }
    found
}

pub(crate) fn unique_title(kind: &str) -> String {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    format!("WinPilot Fixture {kind} {}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))
}

pub(crate) struct TempFiles(pub(crate) Vec<PathBuf>);

impl Drop for TempFiles {
    fn drop(&mut self) {
        for path in &self.0 {
            let _ = std::fs::remove_file(path);
        }
    }
}

pub(crate) fn temp_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("winpilot-pick-{}-{name}", std::process::id()))
}

pub(crate) fn temp_file(name: &str) -> PathBuf {
    let path = temp_path(name);
    std::fs::write(&path, b"winpilot").unwrap();
    path
}

pub(crate) struct ClosesTitled(pub(crate) String);

impl Drop for ClosesTitled {
    fn drop(&mut self) {
        let Ok(windows) = winpilot_desktop::windows() else { return };
        for window in windows.iter().filter(|window| window.title == self.0) {
            let _closed = winpilot_desktop::close_window(window.hwnd);
        }
    }
}

pub(crate) struct FixtureApp {
    pub(crate) window: WindowInfo,
    _closes: ClosesTitled,
    process: FixtureProcess,
}

impl FixtureApp {
    pub(crate) fn launch() -> Option<Self> {
        Self::launch_at(unique_title("Worker"), Placement::OffScreen)
    }

    pub(crate) fn launch_titled(title: String) -> Option<Self> {
        Self::launch_at(title, Placement::OffScreen)
    }

    pub(crate) fn launch_on_screen() -> Option<Self> {
        Self::launch_at(unique_title("Worker"), Placement::OnScreen)
    }

    fn launch_at(title: String, placement: Placement) -> Option<Self> {
        let exe = fixture_exe()?;
        let process = FixtureProcess::launch(&exe, &title, placement)?;
        let closes = ClosesTitled(title);
        let window = wait_for_window(process.pid(), &closes.0)?;
        engine().changes.wait_quiet(process.pid(), QUIET, PATIENCE);
        Some(Self { window, _closes: closes, process })
    }

    pub(crate) fn title(&self) -> &str {
        &self.window.title
    }

    pub(crate) fn hwnd(&self) -> WindowRef {
        self.window.hwnd
    }

    pub(crate) fn current_window(&self) -> WindowInfo {
        winpilot_desktop::window(self.hwnd()).unwrap()
    }

    pub(crate) fn selector(&self, terms: &str) -> String {
        format!("window:\"{}\" {terms}", self.title())
    }

    pub(crate) fn text(&self, control: i32) -> String {
        let Ok(child) = (unsafe { GetDlgItem(Some(self.process.hwnd()), control) }) else { return String::new() };
        let mut buffer = [0u16; 4096];
        let mut copied = 0usize;
        let sent = unsafe {
            SendMessageTimeoutW(
                child,
                WM_GETTEXT,
                WPARAM(buffer.len()),
                LPARAM(buffer.as_mut_ptr() as isize),
                SMTO_ABORTIFHUNG,
                SEND_TIMEOUT_MS,
                Some(&raw mut copied),
            )
        };
        let length = if sent.0 == 0 { 0 } else { copied.min(buffer.len()) };
        String::from_utf16_lossy(&buffer[..length])
    }

    pub(crate) fn settled_text(&self, control: i32, done: impl Fn(&str) -> bool) -> String {
        let deadline = Instant::now() + PATIENCE;
        loop {
            let text = self.text(control);
            if done(&text) || Instant::now() >= deadline {
                return text;
            }
            thread::sleep(POLL);
        }
    }

    pub(crate) fn wait_for_text(&self, control: i32, expected: &str) -> bool {
        self.settled_text(control, |text| text == expected) == expected
    }
}

fn wait_for_window(pid: u32, title: &str) -> Option<WindowInfo> {
    let deadline = Instant::now() + PATIENCE;
    while Instant::now() < deadline {
        let found = winpilot_desktop::windows().ok()?.into_iter().find(|window| window.pid == pid);
        if let Some(window) = found.filter(|window| window.title == title) {
            return Some(window);
        }
        thread::sleep(POLL);
    }
    eprintln!("skipping: the fixture window never appeared");
    None
}
