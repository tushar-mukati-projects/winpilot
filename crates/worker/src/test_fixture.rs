use std::{
    path::PathBuf,
    sync::{
        OnceLock,
        atomic::{AtomicU32, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use deckhand_core::{
    Config, Element, ElementRef, ElementState, Rect, Role, Snapshot, WindowInfo, WindowRef, WindowState,
};
use deckhand_fixture::{FixtureProcess, Placement};
use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    UI::WindowsAndMessaging::{GetDlgItem, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_GETTEXT},
};

use crate::engine::Engine;

const PATIENCE: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(25);
const QUIET: Duration = Duration::from_millis(200);
const SEND_TIMEOUT_MS: u32 = 2000;

pub fn engine() -> &'static Engine {
    static ENGINE: OnceLock<Engine> = OnceLock::new();
    ENGINE.get_or_init(|| {
        let _ = deckhand_desktop::enable_dpi_awareness();
        let mut config = Config::default();
        config.overlay.enabled = false;
        config.guard.kill_hotkey = String::new();
        config.guard.yield_ms = 0;
        Engine::start(config, 0, "deckhand-test").unwrap()
    })
}

pub fn window(hwnd: i64, process: &str, title: &str, rect: Rect) -> WindowInfo {
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

pub fn element(id: u32, depth: u16, role: Role, name: &str) -> Element {
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
        patterns: deckhand_core::Patterns::default(),
    }
}

pub fn with_id(element: Element, automation_id: &str) -> Element {
    Element { automation_id: automation_id.to_string(), ..element }
}

pub fn snapshot(title: &str, elements: Vec<Element>) -> Snapshot {
    let window = window(0x10, "app.exe", title, Rect::new(0, 0, 800, 600));
    Snapshot { window, elements, focused: None, truncated: false }
}

pub fn fixture_exe() -> Option<PathBuf> {
    let found = deckhand_fixture::fixture_exe();
    if found.is_none() {
        eprintln!("skipping: deckhand-fixture.exe is not built (cargo build -p deckhand-fixture)");
    }
    found
}

pub fn unique_title(kind: &str) -> String {
    static NEXT: AtomicU32 = AtomicU32::new(1);
    format!("Deckhand Fixture {kind} {}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed))
}

pub struct ClosesTitled(pub String);

impl Drop for ClosesTitled {
    fn drop(&mut self) {
        let Ok(windows) = deckhand_desktop::windows() else { return };
        for window in windows.iter().filter(|window| window.title == self.0) {
            let _closed = deckhand_desktop::close_window(window.hwnd);
        }
    }
}

pub struct FixtureApp {
    pub window: WindowInfo,
    _closes: ClosesTitled,
    process: FixtureProcess,
}

impl FixtureApp {
    pub fn launch() -> Option<Self> {
        Self::launch_at(unique_title("Worker"), Placement::OffScreen)
    }

    pub fn launch_titled(title: String) -> Option<Self> {
        Self::launch_at(title, Placement::OffScreen)
    }

    pub fn launch_on_screen() -> Option<Self> {
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

    pub fn title(&self) -> &str {
        &self.window.title
    }

    pub fn hwnd(&self) -> WindowRef {
        self.window.hwnd
    }

    pub fn current_window(&self) -> WindowInfo {
        deckhand_desktop::window(self.hwnd()).unwrap()
    }

    pub fn selector(&self, terms: &str) -> String {
        format!("window:\"{}\" {terms}", self.title())
    }

    pub fn text(&self, control: i32) -> String {
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

    pub fn wait_for_text(&self, control: i32, expected: &str) -> bool {
        let deadline = Instant::now() + PATIENCE;
        while Instant::now() < deadline {
            if self.text(control) == expected {
                return true;
            }
            thread::sleep(POLL);
        }
        false
    }
}

fn wait_for_window(pid: u32, title: &str) -> Option<WindowInfo> {
    let deadline = Instant::now() + PATIENCE;
    while Instant::now() < deadline {
        let found = deckhand_desktop::windows().ok()?.into_iter().find(|window| window.pid == pid);
        if let Some(window) = found.filter(|window| window.title == title) {
            return Some(window);
        }
        thread::sleep(POLL);
    }
    eprintln!("skipping: the fixture window never appeared");
    None
}
