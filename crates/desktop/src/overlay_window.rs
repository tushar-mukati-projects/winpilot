use std::{
    mem::size_of,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::Receiver,
    },
};

use deckhand_core::{Context, Point, Rect, Result};
use windows::{
    Win32::{
        Foundation::{ERROR_CLASS_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext},
            WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, HTTRANSPARENT, HWND_TOPMOST, MA_NOACTIVATE, MSG,
                RegisterClassExW, SW_HIDE, SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
                SetWindowDisplayAffinity, SetWindowPos, ShowWindow, WDA_EXCLUDEFROMCAPTURE, WM_APP, WM_MOUSEACTIVATE,
                WM_NCHITTEST, WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
                WS_EX_TRANSPARENT, WS_POPUP,
            },
        },
    },
    core::{PCWSTR, w},
};

use crate::{
    edge_strips::{EDGE_COUNT, border_strips},
    layered_window::present,
    message_thread::ThreadMessages,
    monitors::virtual_screen,
    overlay_paint::{ACCENT, paint_disc, premultiplied},
    window_timer::WindowTimer,
};

pub(crate) const WAKE_OVERLAY: u32 = WM_APP + 0x0D;
const OVERLAY_CLASS: PCWSTR = w!("DeckhandOverlay");
const OVERLAY_TITLE: PCWSTR = w!("Deckhand overlay");
const BORDER_THICKNESS: i32 = 3;
const BORDER_ALPHA: u8 = 200;
const RIPPLE_SIZE: i32 = 28;
const RIPPLE_ALPHA: u8 = 150;
const RIPPLE_TIMER: usize = 1;
const RIPPLE_LIFETIME_MS: u32 = 450;

static CLASS_REGISTERED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OverlayCommand {
    ShowTarget(Rect),
    FlashPoint(Point),
    Hide,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OverlayStatus {
    pub(crate) capture_excluded: bool,
}

struct OverlayWindow(HWND);

impl OverlayWindow {
    fn create(instance: HINSTANCE) -> Result<Self> {
        let style = WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
        let hwnd = unsafe {
            CreateWindowExW(style, OVERLAY_CLASS, OVERLAY_TITLE, WS_POPUP, 0, 0, 0, 0, None, None, Some(instance), None)
        }
        .context("creating an overlay window")?;
        Ok(Self(hwnd))
    }

    fn exclude_from_capture(&self) -> bool {
        unsafe { SetWindowDisplayAffinity(self.0, WDA_EXCLUDEFROMCAPTURE) }.is_ok()
    }

    fn show_strip(&self, strip: Option<Rect>, pixel: u32) {
        let painted = strip.is_some_and(|rect| {
            present(self.0, Point::new(rect.x, rect.y), rect.width, rect.height, |pixels| pixels.fill(pixel)).is_ok()
        });
        if painted { self.show_on_top() } else { self.hide() }
    }

    fn show_on_top(&self) {
        let _was_visible = unsafe { ShowWindow(self.0, SW_SHOWNOACTIVATE) };
        let flags = SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE;
        let _raised = unsafe { SetWindowPos(self.0, Some(HWND_TOPMOST), 0, 0, 0, 0, flags) };
    }

    fn hide(&self) {
        let _was_visible = unsafe { ShowWindow(self.0, SW_HIDE) };
    }
}

impl Drop for OverlayWindow {
    fn drop(&mut self) {
        let _ = unsafe { DestroyWindow(self.0) };
    }
}

pub(crate) struct OverlayWindows {
    ripple_timer: Option<WindowTimer>,
    edges: [OverlayWindow; EDGE_COUNT],
    ripple: OverlayWindow,
    commands: Receiver<OverlayCommand>,
}

impl OverlayWindows {
    pub(crate) fn create(commands: Receiver<OverlayCommand>) -> Result<(Self, OverlayStatus)> {
        let _previous_awareness = unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        let instance = module_instance()?;
        ensure_class_registered(instance)?;
        let edges = [
            OverlayWindow::create(instance)?,
            OverlayWindow::create(instance)?,
            OverlayWindow::create(instance)?,
            OverlayWindow::create(instance)?,
        ];
        let ripple = OverlayWindow::create(instance)?;
        let exposed = edges.iter().chain([&ripple]).filter(|window| !window.exclude_from_capture()).count();
        let status = OverlayStatus { capture_excluded: exposed == 0 };
        Ok((Self { ripple_timer: None, edges, ripple, commands }, status))
    }

    fn apply_pending(&mut self) {
        while let Ok(command) = self.commands.try_recv() {
            match command {
                OverlayCommand::ShowTarget(rect) => self.show_target(rect),
                OverlayCommand::FlashPoint(point) => self.flash_point(point),
                OverlayCommand::Hide => self.hide_all(),
            }
        }
    }

    fn show_target(&self, target: Rect) {
        let pixel = premultiplied(ACCENT, BORDER_ALPHA);
        let strips = border_strips(target, BORDER_THICKNESS, virtual_screen());
        for (edge, strip) in self.edges.iter().zip(strips) {
            edge.show_strip(strip, pixel);
        }
    }

    fn flash_point(&mut self, point: Point) {
        self.ripple_timer = None;
        let origin = Point::new(point.x.saturating_sub(RIPPLE_SIZE / 2), point.y.saturating_sub(RIPPLE_SIZE / 2));
        let painted = present(self.ripple.0, origin, RIPPLE_SIZE, RIPPLE_SIZE, |pixels| {
            paint_disc(pixels, RIPPLE_SIZE as usize, ACCENT, RIPPLE_ALPHA);
        });
        if painted.is_err() {
            self.ripple.hide();
            return;
        }
        self.ripple.show_on_top();
        self.ripple_timer = WindowTimer::start(self.ripple.0, RIPPLE_TIMER, RIPPLE_LIFETIME_MS);
        if self.ripple_timer.is_none() {
            self.ripple.hide();
        }
    }

    fn end_ripple(&mut self) {
        self.ripple_timer = None;
        self.ripple.hide();
    }

    fn hide_all(&mut self) {
        for edge in &self.edges {
            edge.hide();
        }
        self.end_ripple();
    }

    fn is_ripple_timer(&self, message: &MSG) -> bool {
        self.ripple_timer.as_ref().is_some_and(|timer| timer.fired(message.hwnd, message.wParam.0))
    }
}

impl ThreadMessages for OverlayWindows {
    fn on_message(&mut self, message: &MSG) -> bool {
        if message.message == WAKE_OVERLAY && message.hwnd.is_invalid() {
            self.apply_pending();
            return true;
        }
        if message.message == WM_TIMER && self.is_ripple_timer(message) {
            self.end_ripple();
            return true;
        }
        false
    }
}

fn module_instance() -> Result<HINSTANCE> {
    let module = unsafe { GetModuleHandleW(PCWSTR::null()) }.context("finding this module")?;
    Ok(HINSTANCE(module.0))
}

fn ensure_class_registered(instance: HINSTANCE) -> Result<()> {
    if CLASS_REGISTERED.load(Ordering::Acquire) {
        return Ok(());
    }
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(overlay_window_proc),
        hInstance: instance,
        lpszClassName: OVERLAY_CLASS,
        ..WNDCLASSEXW::default()
    };
    let atom = unsafe { RegisterClassExW(&raw const class) };
    if atom == 0 {
        let error = windows::core::Error::from_thread();
        if error.code() != ERROR_CLASS_ALREADY_EXISTS.to_hresult() {
            return Err(error).context("registering the overlay window class");
        }
    }
    CLASS_REGISTERED.store(true, Ordering::Release);
    Ok(())
}

unsafe extern "system" fn overlay_window_proc(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match message {
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}
