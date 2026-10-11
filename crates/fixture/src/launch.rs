use std::{
    path::{Path, PathBuf},
    thread::sleep,
    time::{Duration, Instant},
};

use windows::{
    Win32::{
        Foundation::{HANDLE, HWND, LPARAM, WAIT_OBJECT_0, WPARAM},
        System::Threading::{
            CreateProcessW, GetExitCodeProcess, GetProcessId, PROCESS_CREATION_FLAGS, PROCESS_INFORMATION,
            STARTF_USESHOWWINDOW, STARTUPINFOW, TerminateProcess, WaitForInputIdle, WaitForSingleObject,
        },
        UI::WindowsAndMessaging::{
            FindWindowW, GetSystemMetrics, PostMessageW, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
            SM_YVIRTUALSCREEN, SW_SHOWNOACTIVATE, WM_CLOSE,
        },
    },
    core::{HSTRING, Owned, PCWSTR, PWSTR, w},
};

const CLASS_NAME: PCWSTR = w!("DeckhandFixtureWindow");
const PATIENCE: Duration = Duration::from_secs(10);
const POLL: Duration = Duration::from_millis(25);
const EXIT_WAIT_MS: u32 = 5000;
const ON_SCREEN: (i32, i32) = (120, 120);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    OffScreen,
    OnScreen,
}

pub struct FixtureProcess {
    process: Owned<HANDLE>,
    window: HWND,
}

impl FixtureProcess {
    pub fn launch(exe: &Path, title: &str, placement: Placement) -> Option<Self> {
        let process = spawn_without_activation(exe, title, position(placement))?;
        let _idle = unsafe { WaitForInputIdle(*process, PATIENCE.as_millis() as u32) };
        let window = wait_for_window(title)?;
        Some(Self { process, window })
    }

    pub const fn hwnd(&self) -> HWND {
        self.window
    }

    pub fn pid(&self) -> u32 {
        unsafe { GetProcessId(*self.process) }
    }

    pub fn close(&self) -> Option<u32> {
        unsafe { PostMessageW(Some(self.window), WM_CLOSE, WPARAM(0), LPARAM(0)) }.ok()?;
        if unsafe { WaitForSingleObject(*self.process, EXIT_WAIT_MS) } != WAIT_OBJECT_0 {
            return None;
        }
        let mut code = u32::MAX;
        unsafe { GetExitCodeProcess(*self.process, &raw mut code) }.ok()?;
        Some(code)
    }
}

impl Drop for FixtureProcess {
    fn drop(&mut self) {
        if unsafe { WaitForSingleObject(*self.process, 0) } == WAIT_OBJECT_0 {
            return;
        }
        let _posted = unsafe { PostMessageW(Some(self.window), WM_CLOSE, WPARAM(0), LPARAM(0)) };
        if unsafe { WaitForSingleObject(*self.process, EXIT_WAIT_MS) } != WAIT_OBJECT_0 {
            let _terminated = unsafe { TerminateProcess(*self.process, 1) };
        }
    }
}

pub fn fixture_exe() -> Option<PathBuf> {
    let exe = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/deckhand-fixture.exe");
    exe.exists().then_some(exe)
}

fn position(placement: Placement) -> (i32, i32) {
    match placement {
        Placement::OnScreen => ON_SCREEN,
        Placement::OffScreen => beyond_the_virtual_screen(),
    }
}

fn beyond_the_virtual_screen() -> (i32, i32) {
    let right = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) + GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
    (right.max(1).saturating_add(200), top.saturating_add(height / 8))
}

fn wait_for_window(title: &str) -> Option<HWND> {
    let title = HSTRING::from(title);
    let deadline = Instant::now() + PATIENCE;
    while Instant::now() < deadline {
        if let Ok(window) = unsafe { FindWindowW(CLASS_NAME, &title) } {
            return Some(window);
        }
        sleep(POLL);
    }
    None
}

fn spawn_without_activation(exe: &Path, title: &str, (x, y): (i32, i32)) -> Option<Owned<HANDLE>> {
    let command = format!("\"{}\" --title \"{title}\" --pos {x},{y}", exe.display());
    let mut command: Vec<u16> = command.encode_utf16().chain(Some(0)).collect();
    let startup = STARTUPINFOW {
        cb: size_of::<STARTUPINFOW>() as u32,
        dwFlags: STARTF_USESHOWWINDOW,
        wShowWindow: SW_SHOWNOACTIVATE.0 as u16,
        ..Default::default()
    };
    let mut info = PROCESS_INFORMATION::default();
    let created = unsafe {
        CreateProcessW(
            PCWSTR::null(),
            Some(PWSTR(command.as_mut_ptr())),
            None,
            None,
            false,
            PROCESS_CREATION_FLAGS(0),
            None,
            PCWSTR::null(),
            &raw const startup,
            &raw mut info,
        )
    };
    created.ok()?;
    drop(unsafe { Owned::new(info.hThread) });
    Some(unsafe { Owned::new(info.hProcess) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn off_screen_starts_right_of_every_monitor() {
        let (x, _) = position(Placement::OffScreen);
        let right = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) + GetSystemMetrics(SM_CXVIRTUALSCREEN) };
        assert!(x > right);
        assert_eq!(position(Placement::OnScreen), ON_SCREEN);
    }
}
