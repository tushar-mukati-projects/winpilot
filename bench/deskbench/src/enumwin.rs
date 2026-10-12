use std::collections::HashMap;
use std::ffi::c_void;

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::System::Threading::{OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{BOOL, PWSTR};

use crate::util::wide_to_string;

#[derive(Clone, Debug)]
pub struct WinInfo {
    pub hwnd: isize,
    pub z: usize,
    pub title: String,
    pub class: String,
    pub process: String,
    pub rect: RECT,
    pub minimized: bool,
    pub cloaked: bool,
    pub toolwindow: bool,
}

impl WinInfo {
    pub fn area(&self) -> i64 {
        (self.rect.right - self.rect.left).max(0) as i64 * (self.rect.bottom - self.rect.top).max(0) as i64
    }

    pub fn is_app_window(&self) -> bool {
        !self.cloaked && !self.toolwindow && !self.title.is_empty() && self.area() > 0
    }
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let v = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
    v.push(hwnd);
    BOOL(1)
}

pub fn process_name(pid: u32) -> String {
    unsafe {
        let Ok(h) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return format!("pid{pid}");
        };
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        if !ok {
            return format!("pid{pid}");
        }
        let full = String::from_utf16_lossy(&buf[..len as usize]);
        full.rsplit('\\').next().unwrap_or(&full).to_string()
    }
}

pub fn enumerate(pid_cache: &mut HashMap<u32, String>) -> Vec<WinInfo> {
    let mut hwnds: Vec<HWND> = Vec::with_capacity(1024);
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut hwnds as *mut Vec<HWND> as isize));
    }
    let mut out = Vec::with_capacity(128);
    let mut tbuf = [0u16; 512];
    let mut cbuf = [0u16; 256];
    for (z, &hwnd) in hwnds.iter().enumerate() {
        unsafe {
            if !IsWindowVisible(hwnd).as_bool() {
                continue;
            }
            let mut cloaked: u32 = 0;
            let _ = DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, &mut cloaked as *mut u32 as *mut c_void, 4);
            let tl = GetWindowTextW(hwnd, &mut tbuf);
            let title = if tl > 0 { String::from_utf16_lossy(&tbuf[..tl as usize]) } else { String::new() };
            let cl = GetClassNameW(hwnd, &mut cbuf);
            let class = wide_to_string(&cbuf[..cl.max(0) as usize]);
            let mut rect = RECT::default();
            let _ = GetWindowRect(hwnd, &mut rect);
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            let process = pid_cache.entry(pid).or_insert_with(|| process_name(pid)).clone();
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            out.push(WinInfo {
                hwnd: hwnd.0 as isize,
                z,
                title,
                class,
                process,
                rect,
                minimized: IsIconic(hwnd).as_bool(),
                cloaked: cloaked != 0,
                toolwindow: ex & WS_EX_TOOLWINDOW.0 != 0,
            });
        }
    }
    out
}

pub fn foreground() -> HWND {
    unsafe { GetForegroundWindow() }
}
