use std::mem::size_of;

use deckhand_core::{Error, Point, Rect, Result};
use windows::Win32::{
    Foundation::{COLORREF, HWND, POINT, RECT, SIZE},
    Graphics::Gdi::{AC_SRC_ALPHA, AC_SRC_OVER, BLENDFUNCTION, HDC},
    UI::WindowsAndMessaging::{ULW_ALPHA, UPDATELAYEREDWINDOWINFO, UpdateLayeredWindow, UpdateLayeredWindowIndirect},
};

const MAX_LAYER_SIDE: i32 = 32_767;
const MAX_LAYER_PIXELS: i64 = 1 << 22;

pub(crate) fn is_layer_size_allowed(width: i32, height: i32) -> bool {
    let sides_fit = (1..=MAX_LAYER_SIDE).contains(&width) && (1..=MAX_LAYER_SIDE).contains(&height);
    sides_fit && i64::from(width) * i64::from(height) <= MAX_LAYER_PIXELS
}

pub(crate) fn update_layer(hwnd: HWND, source: HDC, placement: Rect, alpha: u8, dirty: Option<Rect>) -> Result<()> {
    if !is_layer_size_allowed(placement.width, placement.height) {
        return Err(Error::invalid(format!("overlay size {}x{} is out of range", placement.width, placement.height)));
    }
    let destination = POINT { x: placement.x, y: placement.y };
    let size = SIZE { cx: placement.width, cy: placement.height };
    let origin = POINT { x: 0, y: 0 };
    let blend = blend(alpha);
    let dirty = dirty.map(|area| RECT { left: area.x, top: area.y, right: area.right(), bottom: area.bottom() });
    let info = UPDATELAYEREDWINDOWINFO {
        cbSize: size_of::<UPDATELAYEREDWINDOWINFO>() as u32,
        pptDst: &raw const destination,
        psize: &raw const size,
        hdcSrc: source,
        pptSrc: &raw const origin,
        crKey: COLORREF(0),
        pblend: &raw const blend,
        dwFlags: ULW_ALPHA,
        prcDirty: dirty.as_ref().map_or(std::ptr::null(), std::ptr::from_ref),
        ..UPDATELAYEREDWINDOWINFO::default()
    };
    if unsafe { UpdateLayeredWindowIndirect(hwnd, &raw const info) }.as_bool() {
        return Ok(());
    }
    Err(Error::internal(format!("updating an overlay window: {}", windows::core::Error::from_thread())))
}

pub(crate) fn reposition_layer(hwnd: HWND, origin: Option<Point>, alpha: u8) -> Result<()> {
    let destination = origin.map(|point| POINT { x: point.x, y: point.y });
    let blend = blend(alpha);
    unsafe {
        UpdateLayeredWindow(
            hwnd,
            None,
            destination.as_ref().map(std::ptr::from_ref),
            None,
            None,
            None,
            COLORREF(0),
            Some(&raw const blend),
            ULW_ALPHA,
        )
    }
    .map_err(|error| Error::internal(format!("moving an overlay window: {error}")))
}

fn blend(alpha: u8) -> BLENDFUNCTION {
    BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: alpha,
        AlphaFormat: AC_SRC_ALPHA as u8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_thin_strips_and_small_ripples_are_allowed() {
        assert!(is_layer_size_allowed(32_767, 3));
        assert!(is_layer_size_allowed(3, 20_000));
        assert!(is_layer_size_allowed(28, 28));
    }

    #[test]
    fn empty_oversized_or_huge_layers_are_refused() {
        assert!(!is_layer_size_allowed(0, 3));
        assert!(!is_layer_size_allowed(40_000, 3));
        assert!(!is_layer_size_allowed(3_840, 2_160));
        assert!(!is_layer_size_allowed(-1, -1));
    }

    #[test]
    fn a_null_window_is_refused() {
        assert!(update_layer(HWND::default(), HDC::default(), Rect::new(0, 0, 4, 4), 255, None).is_err());
        assert!(update_layer(HWND::default(), HDC::default(), Rect::new(0, 0, 0, 4), 255, None).is_err());
        assert!(reposition_layer(HWND::default(), None, 255).is_err());
    }
}
