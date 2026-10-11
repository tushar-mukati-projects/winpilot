use std::{ffi::c_void, mem::size_of, ptr};

use deckhand_core::{Context, Error, Point, Result};
use windows::Win32::{
    Foundation::{COLORREF, HWND, POINT, SIZE},
    Graphics::Gdi::{
        AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, CreateCompatibleDC,
        CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, HBITMAP, HDC, HGDIOBJ, SelectObject,
    },
    UI::WindowsAndMessaging::{ULW_ALPHA, UpdateLayeredWindow},
};

const MAX_LAYER_SIDE: i32 = 32_767;
const MAX_LAYER_PIXELS: i64 = 1 << 22;

struct MemoryDc(HDC);

impl MemoryDc {
    fn new() -> Result<Self> {
        let dc = unsafe { CreateCompatibleDC(None) };
        if dc.is_invalid() {
            return Err(Error::internal("creating a memory device context"));
        }
        Ok(Self(dc))
    }
}

impl Drop for MemoryDc {
    fn drop(&mut self) {
        let _deleted = unsafe { DeleteDC(self.0) };
    }
}

struct DibSection {
    bitmap: HBITMAP,
    bits: *mut c_void,
}

impl DibSection {
    fn new(dc: &MemoryDc, width: i32, height: i32) -> Result<Self> {
        let info = top_down_bgra(width, height);
        let mut bits: *mut c_void = ptr::null_mut();
        let bitmap = unsafe { CreateDIBSection(Some(dc.0), &raw const info, DIB_RGB_COLORS, &raw mut bits, None, 0) }
            .context("creating the overlay bitmap")?;
        let section = Self { bitmap, bits };
        if section.bits.is_null() {
            return Err(Error::internal("the overlay bitmap has no pixels"));
        }
        Ok(section)
    }

    fn pixels(&mut self, count: usize) -> &mut [u32] {
        unsafe { std::slice::from_raw_parts_mut(self.bits.cast::<u32>(), count) }
    }
}

impl Drop for DibSection {
    fn drop(&mut self) {
        let _deleted = unsafe { DeleteObject(HGDIOBJ(self.bitmap.0)) };
    }
}

struct Selection<'a> {
    dc: &'a MemoryDc,
    previous: HGDIOBJ,
}

impl<'a> Selection<'a> {
    fn new(dc: &'a MemoryDc, section: &DibSection) -> Result<Self> {
        let previous = unsafe { SelectObject(dc.0, HGDIOBJ(section.bitmap.0)) };
        if previous.is_invalid() {
            return Err(Error::internal("selecting the overlay bitmap"));
        }
        Ok(Self { dc, previous })
    }
}

impl Drop for Selection<'_> {
    fn drop(&mut self) {
        let _restored = unsafe { SelectObject(self.dc.0, self.previous) };
    }
}

fn top_down_bgra(width: i32, height: i32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..BITMAPINFOHEADER::default()
        },
        ..BITMAPINFO::default()
    }
}

pub(crate) fn present(
    hwnd: HWND,
    origin: Point,
    width: i32,
    height: i32,
    paint: impl FnOnce(&mut [u32]),
) -> Result<()> {
    if !is_layer_size_allowed(width, height) {
        return Err(Error::invalid(format!("overlay size {width}x{height} is out of range")));
    }
    let dc = MemoryDc::new()?;
    let mut section = DibSection::new(&dc, width, height)?;
    let pixels = section.pixels(width as usize * height as usize);
    pixels.fill(0);
    paint(pixels);
    let _selection = Selection::new(&dc, &section)?;
    update_layer(hwnd, &dc, origin, SIZE { cx: width, cy: height })
}

fn is_layer_size_allowed(width: i32, height: i32) -> bool {
    let sides_fit = (1..=MAX_LAYER_SIDE).contains(&width) && (1..=MAX_LAYER_SIDE).contains(&height);
    sides_fit && i64::from(width) * i64::from(height) <= MAX_LAYER_PIXELS
}

fn update_layer(hwnd: HWND, dc: &MemoryDc, origin: Point, size: SIZE) -> Result<()> {
    let destination = POINT { x: origin.x, y: origin.y };
    let source = POINT { x: 0, y: 0 };
    let blend = BLENDFUNCTION {
        BlendOp: AC_SRC_OVER as u8,
        BlendFlags: 0,
        SourceConstantAlpha: 255,
        AlphaFormat: AC_SRC_ALPHA as u8,
    };
    unsafe {
        UpdateLayeredWindow(
            hwnd,
            None,
            Some(&raw const destination),
            Some(&raw const size),
            Some(dc.0),
            Some(&raw const source),
            COLORREF(0),
            Some(&raw const blend),
            ULW_ALPHA,
        )
    }
    .context("updating the overlay window")
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
}
