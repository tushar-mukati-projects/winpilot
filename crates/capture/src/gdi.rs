use std::ffi::c_void;

use deckhand_core::{Context, Error, Point, Result};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, GdiFlush,
    GetDC, HBITMAP, HDC, HGDIOBJ, ReleaseDC, SelectObject,
};
use windows_core::Owned;

use crate::frame::{Frame, FrameSource};
use crate::pixels::{BYTES_PER_PIXEL, Pixels};

pub(crate) struct ScreenDc(HDC);

impl ScreenDc {
    pub(crate) fn acquire() -> Result<Self> {
        let dc = unsafe { GetDC(None) };
        if dc.is_invalid() {
            return Err(Error::internal("GetDC for the screen failed"));
        }
        Ok(Self(dc))
    }

    pub(crate) const fn dc(&self) -> HDC {
        self.0
    }
}

impl Drop for ScreenDc {
    fn drop(&mut self) {
        unsafe { ReleaseDC(None, self.0) };
    }
}

struct MemoryDc(HDC);

impl MemoryDc {
    fn create() -> Result<Self> {
        let dc = unsafe { CreateCompatibleDC(None) };
        if dc.is_invalid() {
            return Err(Error::internal("CreateCompatibleDC failed"));
        }
        Ok(Self(dc))
    }
}

impl Drop for MemoryDc {
    fn drop(&mut self) {
        let _ = unsafe { DeleteDC(self.0) };
    }
}

pub(crate) struct Selection {
    dc: HDC,
    previous: HGDIOBJ,
}

impl Selection {
    pub(crate) fn select(dc: HDC, object: HGDIOBJ) -> Result<Self> {
        let previous = unsafe { SelectObject(dc, object) };
        if previous.is_invalid() {
            return Err(Error::internal("SelectObject failed"));
        }
        Ok(Self { dc, previous })
    }
}

impl Drop for Selection {
    fn drop(&mut self) {
        unsafe { SelectObject(self.dc, self.previous) };
    }
}

pub(crate) struct GdiSurface {
    _selection: Selection,
    _bitmap: Owned<HBITMAP>,
    bits: *const u8,
    width: u32,
    height: u32,
    dc: MemoryDc,
}

impl GdiSurface {
    pub(crate) fn new(width: u32, height: u32) -> Result<Self> {
        if width == 0 || height == 0 || i32::try_from(width).is_err() || i32::try_from(height).is_err() {
            return Err(Error::invalid("bitmap size is out of range"));
        }
        let dc = MemoryDc::create()?;
        let (bitmap, bits) = create_top_down_dib(&dc, width, height)?;
        let selection = Selection::select(dc.0, HGDIOBJ::from(*bitmap))?;
        Ok(Self { _selection: selection, _bitmap: bitmap, bits, width, height, dc })
    }

    pub(crate) fn dc(&self) -> HDC {
        self.dc.0
    }

    pub(crate) const fn holds(&self, width: u32, height: u32) -> bool {
        self.width >= width && self.height >= height
    }

    pub(crate) const fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub(crate) fn pixels(&self) -> Result<Pixels<'_>> {
        unsafe { GdiFlush() }.ok().context("flushing GDI")?;
        let stride = self.width as usize * BYTES_PER_PIXEL;
        let bytes = unsafe { std::slice::from_raw_parts(self.bits, stride * self.height as usize) };
        Ok(Pixels { bytes, stride })
    }

    pub(crate) fn to_frame(&self, origin: Point, source: FrameSource) -> Result<Frame> {
        let bgra = self.pixels()?.bytes.to_vec();
        Ok(Frame { origin, width: self.width, height: self.height, bgra, source })
    }
}

fn create_top_down_dib(dc: &MemoryDc, width: u32, height: u32) -> Result<(Owned<HBITMAP>, *const u8)> {
    let mut info = BITMAPINFO::default();
    info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = width as i32;
    info.bmiHeader.biHeight = -(height as i32);
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB.0;
    let mut bits: *mut c_void = std::ptr::null_mut();
    let bitmap = unsafe { CreateDIBSection(Some(dc.0), &raw const info, DIB_RGB_COLORS, &raw mut bits, None, 0) }
        .context("CreateDIBSection")?;
    let bitmap = unsafe { Owned::new(bitmap) };
    if bits.is_null() {
        return Err(Error::internal("CreateDIBSection returned no pixel memory"));
    }
    Ok((bitmap, bits.cast_const().cast::<u8>()))
}

#[cfg(test)]
mod tests {
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{FillRect, GetStockObject, HBRUSH, WHITE_BRUSH};

    use super::*;

    #[test]
    fn surface_is_top_down_bgra() {
        let surface = GdiSurface::new(8, 4).unwrap();
        let brush = HBRUSH(unsafe { GetStockObject(WHITE_BRUSH) }.0);
        let top_row = RECT { left: 0, top: 0, right: 8, bottom: 1 };
        assert_ne!(unsafe { FillRect(surface.dc(), &raw const top_row, brush) }, 0);
        let frame = surface.to_frame(Point::new(3, 4), FrameSource::Window).unwrap();
        assert_eq!((frame.width, frame.height, frame.bgra.len()), (8, 4, 128));
        assert_eq!(frame.source, FrameSource::Window);
        assert_eq!(&frame.bgra[..3], &[255, 255, 255]);
        assert_eq!(&frame.bgra[32..35], &[0, 0, 0]);
    }

    #[test]
    fn zero_sized_surfaces_are_rejected() {
        assert!(GdiSurface::new(0, 10).is_err());
    }

    #[test]
    fn surfaces_report_what_they_can_hold() {
        let surface = GdiSurface::new(8, 4).unwrap();
        assert_eq!(surface.size(), (8, 4));
        assert!(surface.holds(8, 4) && surface.holds(1, 1));
        assert!(!surface.holds(9, 4) && !surface.holds(8, 5));
    }
}
