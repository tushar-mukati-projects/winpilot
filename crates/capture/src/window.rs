use deckhand_core::{Context, Error, ErrorCode, Point, Rect, Result, WindowRef};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect, IsHungAppWindow, IsIconic, IsWindow, IsWindowVisible, PW_RENDERFULLCONTENT,
};

use crate::convert::{hwnd_from_ref, rect_from_win32};
use crate::dpi::with_window_dpi;
use crate::frame::{Frame, overlap, validate_capture_rect};
use crate::gdi::GdiSurface;
use crate::resize::resize_bgra;

pub(crate) fn capture_window(window: WindowRef, screen: impl FnOnce(Rect) -> Result<Frame>) -> Result<Frame> {
    let hwnd = live_window(window)?;
    if unsafe { IsIconic(hwnd) }.as_bool() {
        return Err(Error::invalid("window is minimized"));
    }
    let outer = window_rect(hwnd)?;
    validate_capture_rect(outer)?;
    let visible = visible_bounds(hwnd, outer)?;
    match printed_window(hwnd, outer) {
        Some(printed) if !is_all_black(&printed) => {
            printed.crop(visible).ok_or_else(|| Error::internal("window image does not cover its visible bounds"))
        }
        printed => fall_back_to_screen(hwnd, visible, printed, screen),
    }
}

fn live_window(window: WindowRef) -> Result<HWND> {
    let hwnd = hwnd_from_ref(window);
    if hwnd.is_invalid() || !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
        return Err(Error::new(ErrorCode::NotFound, format!("window {window} does not exist")));
    }
    Ok(hwnd)
}

fn window_rect(hwnd: HWND) -> Result<Rect> {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(hwnd, &raw mut rect) }.context("GetWindowRect")?;
    Ok(rect_from_win32(rect))
}

fn visible_bounds(hwnd: HWND, outer: Rect) -> Result<Rect> {
    let mut rect = RECT::default();
    let size = size_of::<RECT>() as u32;
    let queried = unsafe { DwmGetWindowAttribute(hwnd, DWMWA_EXTENDED_FRAME_BOUNDS, (&raw mut rect).cast(), size) };
    let bounds = if queried.is_ok() { rect_from_win32(rect) } else { outer };
    overlap(bounds, outer).ok_or_else(|| Error::invalid("window has no visible area"))
}

fn printed_window(hwnd: HWND, outer: Rect) -> Option<Frame> {
    if unsafe { IsHungAppWindow(hwnd) }.as_bool() {
        return None;
    }
    let printed = with_window_dpi(hwnd, || print_at_window_dpi(hwnd)).ok().flatten()?;
    to_physical_size(printed, outer).ok()
}

fn print_at_window_dpi(hwnd: HWND) -> Result<Option<Frame>> {
    let logical = window_rect(hwnd)?;
    validate_capture_rect(logical)?;
    let surface = GdiSurface::new(logical.width as u32, logical.height as u32)?;
    let flags = PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT);
    if !unsafe { PrintWindow(hwnd, surface.dc(), flags) }.as_bool() {
        return Ok(None);
    }
    surface.to_frame(Point::new(logical.x, logical.y)).map(Some)
}

fn to_physical_size(printed: Frame, outer: Rect) -> Result<Frame> {
    let size = (outer.width as u32, outer.height as u32);
    let bgra = if (printed.width, printed.height) == size {
        printed.bgra
    } else {
        resize_bgra(&printed.bgra, (printed.width, printed.height), size)?
    };
    Ok(Frame { origin: Point::new(outer.x, outer.y), width: size.0, height: size.1, bgra })
}

fn fall_back_to_screen(
    hwnd: HWND,
    visible: Rect,
    printed: Option<Frame>,
    screen: impl FnOnce(Rect) -> Result<Frame>,
) -> Result<Frame> {
    if unsafe { IsWindowVisible(hwnd) }.as_bool() {
        return screen(visible);
    }
    let printed = printed.ok_or_else(|| Error::unsupported("hidden window could not be printed"))?;
    printed.crop(visible).ok_or_else(|| Error::internal("window image does not cover its visible bounds"))
}

fn is_all_black(frame: &Frame) -> bool {
    frame.bgra.as_chunks::<4>().0.iter().all(|pixel| pixel[..3] == [0, 0, 0])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn black_detection_ignores_alpha() {
        let mut frame = Frame::black(Point::new(0, 0), 4, 4);
        frame.bgra.iter_mut().skip(3).step_by(4).for_each(|alpha| *alpha = 255);
        assert!(is_all_black(&frame));
        frame.bgra[20] = 1;
        assert!(!is_all_black(&frame));
    }

    #[test]
    fn physical_resize_keeps_flat_content() {
        let printed = Frame { origin: Point::new(5, 5), width: 4, height: 2, bgra: [9u8, 8, 7, 255].repeat(8) };
        let scaled = to_physical_size(printed, Rect::new(-10, 20, 6, 3)).unwrap();
        assert_eq!((scaled.origin, scaled.width, scaled.height), (Point::new(-10, 20), 6, 3));
        assert!(scaled.bgra.as_chunks::<4>().0.iter().all(|pixel| *pixel == [9, 8, 7, 255]));
    }

    #[test]
    fn missing_windows_are_not_found() {
        let error = capture_window(WindowRef(0), |_| Err(Error::internal("unused"))).unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
        let error = capture_window(WindowRef(0x7FFF_FFF0), |_| Err(Error::internal("unused"))).unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
    }
}
