use std::{ffi::c_void, mem::size_of, ptr};

use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject,
    HBITMAP, HDC, HFONT, HGDIOBJ, SelectObject,
};
use winpilot_core::{Context, Error, Result};

pub(crate) struct MemoryDc(HDC);

impl MemoryDc {
    pub(crate) fn new() -> Result<Self> {
        let dc = unsafe { CreateCompatibleDC(None) };
        if dc.is_invalid() {
            return Err(Error::internal("creating a memory device context"));
        }
        Ok(Self(dc))
    }

    pub(crate) fn handle(&self) -> HDC {
        self.0
    }
}

impl Drop for MemoryDc {
    fn drop(&mut self) {
        let _deleted = unsafe { DeleteDC(self.0) };
    }
}

pub(crate) struct DibSection {
    bitmap: HBITMAP,
    bits: *mut c_void,
    width: usize,
    height: usize,
}

impl DibSection {
    pub(crate) fn new(dc: &MemoryDc, width: i32, height: i32) -> Result<Self> {
        let (Ok(columns), Ok(rows)) = (usize::try_from(width), usize::try_from(height)) else {
            return Err(Error::invalid(format!("bitmap size {width}x{height} is out of range")));
        };
        let info = top_down_bgra(width, height);
        let mut bits: *mut c_void = ptr::null_mut();
        let bitmap = unsafe { CreateDIBSection(Some(dc.0), &raw const info, DIB_RGB_COLORS, &raw mut bits, None, 0) }
            .context("creating an overlay bitmap")?;
        let section = Self { bitmap, bits, width: columns, height: rows };
        if section.bits.is_null() {
            return Err(Error::internal("the overlay bitmap has no pixels"));
        }
        Ok(section)
    }

    pub(crate) const fn width(&self) -> usize {
        self.width
    }

    pub(crate) const fn height(&self) -> usize {
        self.height
    }

    pub(crate) fn pixels(&mut self) -> &mut [u32] {
        unsafe { std::slice::from_raw_parts_mut(self.bits.cast::<u32>(), self.width * self.height) }
    }

    pub(crate) fn object(&self) -> HGDIOBJ {
        HGDIOBJ(self.bitmap.0)
    }
}

impl Drop for DibSection {
    fn drop(&mut self) {
        let _deleted = unsafe { DeleteObject(HGDIOBJ(self.bitmap.0)) };
    }
}

pub(crate) struct Font(HFONT);

impl Font {
    pub(crate) fn adopt(font: HFONT) -> Result<Self> {
        if font.is_invalid() {
            return Err(Error::internal("creating the overlay font"));
        }
        Ok(Self(font))
    }

    pub(crate) fn object(&self) -> HGDIOBJ {
        HGDIOBJ(self.0.0)
    }
}

impl Drop for Font {
    fn drop(&mut self) {
        let _deleted = unsafe { DeleteObject(HGDIOBJ(self.0.0)) };
    }
}

pub(crate) struct Selection<'a> {
    dc: &'a MemoryDc,
    previous: HGDIOBJ,
}

impl<'a> Selection<'a> {
    pub(crate) fn new(dc: &'a MemoryDc, object: HGDIOBJ) -> Result<Self> {
        let previous = unsafe { SelectObject(dc.0, object) };
        if previous.is_invalid() {
            return Err(Error::internal("selecting an object into the overlay device context"));
        }
        Ok(Self { dc, previous })
    }
}

impl Drop for Selection<'_> {
    fn drop(&mut self) {
        let _restored = unsafe { SelectObject(self.dc.0, self.previous) };
    }
}

pub(crate) fn select_kept(dc: &MemoryDc, object: HGDIOBJ) -> Result<HGDIOBJ> {
    let previous = unsafe { SelectObject(dc.0, object) };
    if previous.is_invalid() {
        return Err(Error::internal("selecting the overlay bitmap"));
    }
    Ok(previous)
}

pub(crate) fn restore_selection(dc: &MemoryDc, previous: HGDIOBJ) {
    let _restored = unsafe { SelectObject(dc.0, previous) };
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dib_section_exposes_its_pixels() {
        let dc = MemoryDc::new().unwrap();
        let mut section = DibSection::new(&dc, 8, 4).unwrap();
        assert_eq!((section.width(), section.height()), (8, 4));
        section.pixels().fill(0xFF00_FF00);
        let selection = Selection::new(&dc, section.object()).unwrap();
        drop(selection);
        assert!(section.pixels().iter().all(|pixel| *pixel == 0xFF00_FF00));
    }

    #[test]
    fn negative_sizes_are_refused() {
        let dc = MemoryDc::new().unwrap();
        assert!(DibSection::new(&dc, -1, 4).is_err());
    }
}
