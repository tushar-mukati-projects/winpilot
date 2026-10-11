use deckhand_core::{Point, Rect, Result};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::UI::WindowsAndMessaging::{PW_RENDERFULLCONTENT, SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_NULL};

use crate::capture_area::CaptureArea;
use crate::dpi::with_window_dpi;
use crate::frame::{Frame, FrameSource};
use crate::gdi::GdiSurface;
use crate::resize::resize_bgra;
use crate::window::window_rect;

const RESPONSE_TIMEOUT_MS: u32 = 200;

pub(crate) fn printed_window(hwnd: HWND, outer: Rect) -> Option<Frame> {
    if !responds_promptly(hwnd) {
        return None;
    }
    let printed = with_window_dpi(hwnd, || print_at_window_dpi(hwnd)).ok().flatten()?;
    to_physical_size(printed, outer).ok()
}

fn responds_promptly(hwnd: HWND) -> bool {
    let mut result = 0usize;
    let sent = unsafe {
        SendMessageTimeoutW(
            hwnd,
            WM_NULL,
            WPARAM(0),
            LPARAM(0),
            SMTO_ABORTIFHUNG,
            RESPONSE_TIMEOUT_MS,
            Some(&raw mut result),
        )
    };
    sent.0 != 0
}

fn print_at_window_dpi(hwnd: HWND) -> Result<Option<Frame>> {
    let logical = CaptureArea::new(window_rect(hwnd)?)?.rect();
    let surface = GdiSurface::new(logical.width as u32, logical.height as u32)?;
    let flags = PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT);
    if !unsafe { PrintWindow(hwnd, surface.dc(), flags) }.as_bool() {
        return Ok(None);
    }
    surface.to_frame(Point::new(logical.x, logical.y), FrameSource::Window).map(Some)
}

fn to_physical_size(printed: Frame, outer: Rect) -> Result<Frame> {
    let size = (outer.width as u32, outer.height as u32);
    let bgra = if (printed.width, printed.height) == size {
        printed.bgra
    } else {
        resize_bgra(&printed.bgra, (printed.width, printed.height), size)?
    };
    Ok(Frame { origin: Point::new(outer.x, outer.y), width: size.0, height: size.1, bgra, source: FrameSource::Window })
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use windows::Win32::UI::WindowsAndMessaging::FindWindowW;
    use windows_core::w;

    use super::*;

    #[test]
    fn physical_resize_keeps_flat_content() {
        let mut printed = Frame::black(Point::new(5, 5), 4, 2);
        printed.bgra = [9u8, 8, 7, 255].repeat(8);
        let scaled = to_physical_size(printed, Rect::new(-10, 20, 6, 3)).unwrap();
        assert_eq!((scaled.origin, scaled.width, scaled.height), (Point::new(-10, 20), 6, 3));
        assert_eq!(scaled.source, FrameSource::Window);
        assert!(scaled.bgra.as_chunks::<4>().0.iter().all(|pixel| *pixel == [9, 8, 7, 255]));
    }

    #[test]
    fn invalid_windows_fail_the_responsiveness_probe_quickly() {
        let started = Instant::now();
        assert!(!responds_promptly(HWND(std::ptr::null_mut())));
        assert!(!responds_promptly(HWND(0x7FFF_FFF0 as *mut _)));
        assert!(started.elapsed() < Duration::from_millis(RESPONSE_TIMEOUT_MS.into()));
    }

    #[test]
    fn the_taskbar_responds_and_prints_as_a_window() {
        let Ok(taskbar) = (unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }) else {
            return;
        };
        assert!(responds_promptly(taskbar));
        let outer = window_rect(taskbar).unwrap();
        if let Some(printed) = printed_window(taskbar, outer) {
            assert_eq!((printed.rect(), printed.source), (outer, FrameSource::Window));
        }
    }
}
