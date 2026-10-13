use windows::Win32::Graphics::Gdi::{HDC, HGDIOBJ};
use winpilot_core::{Error, Result};

use super::{
    canvas::Canvas,
    gdi::{DibSection, MemoryDc, restore_selection, select_kept},
};

const SLACK_NUMERATOR: usize = 5;
const SLACK_DENOMINATOR: usize = 4;
const SHRINK_FACTOR: usize = 4;
const SMALL_AREA: usize = 64 * 64;

struct HeldBitmap {
    section: DibSection,
    previous: HGDIOBJ,
}

pub(crate) struct LayerSurface {
    held: Option<HeldBitmap>,
    dc: MemoryDc,
}

impl LayerSurface {
    pub(crate) fn new() -> Result<Self> {
        Ok(Self { held: None, dc: MemoryDc::new()? })
    }

    pub(crate) fn dc(&self) -> HDC {
        self.dc.handle()
    }

    pub(crate) fn canvas(&mut self, width: usize, height: usize) -> Result<Canvas<'_>> {
        if !self.held.as_ref().is_some_and(|held| fits(held, width, height)) {
            self.release();
            self.held = Some(self.allocate(width, height)?);
        }
        let held = self.held.as_mut().ok_or_else(|| Error::internal("the overlay surface has no bitmap"))?;
        let stride = held.section.width();
        Canvas::new(held.section.pixels(), stride, width, height)
            .ok_or_else(|| Error::internal("the overlay surface is smaller than requested"))
    }

    pub(crate) fn release(&mut self) {
        if let Some(held) = self.held.take() {
            restore_selection(&self.dc, held.previous);
        }
    }

    fn allocate(&self, width: usize, height: usize) -> Result<HeldBitmap> {
        let roomy = |length: usize| length.saturating_mul(SLACK_NUMERATOR) / SLACK_DENOMINATOR;
        let columns = i32::try_from(roomy(width).max(1)).map_err(|_| too_big(width, height))?;
        let rows = i32::try_from(roomy(height).max(1)).map_err(|_| too_big(width, height))?;
        let section = DibSection::new(&self.dc, columns, rows)?;
        let previous = select_kept(&self.dc, section.object())?;
        Ok(HeldBitmap { section, previous })
    }
}

impl Drop for LayerSurface {
    fn drop(&mut self) {
        self.release();
    }
}

fn fits(held: &HeldBitmap, width: usize, height: usize) -> bool {
    let (columns, rows) = (held.section.width(), held.section.height());
    let wanted = width.saturating_mul(height).max(SMALL_AREA);
    width <= columns && height <= rows && columns.saturating_mul(rows) <= wanted.saturating_mul(SHRINK_FACTOR)
}

fn too_big(width: usize, height: usize) -> Error {
    Error::invalid(format!("overlay surface {width}x{height} is too large"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bitmap_is_reused_while_the_size_fits() {
        let mut surface = LayerSurface::new().unwrap();
        surface.canvas(100, 20).unwrap().set(99, 19, 7);
        assert_eq!(surface.canvas(90, 18).unwrap().pixel(99, 19), 0);
        assert_eq!(surface.canvas(100, 20).unwrap().pixel(99, 19), 7);
    }

    #[test]
    fn growing_past_the_slack_reallocates() {
        let mut surface = LayerSurface::new().unwrap();
        surface.canvas(100, 20).unwrap().set(0, 0, 7);
        assert_eq!(surface.canvas(400, 20).unwrap().pixel(0, 0), 0);
        let canvas = surface.canvas(400, 20).unwrap();
        assert_eq!((canvas.width(), canvas.height()), (400, 20));
    }

    #[test]
    fn released_surfaces_allocate_again_on_demand() {
        let mut surface = LayerSurface::new().unwrap();
        surface.canvas(10, 10).unwrap();
        surface.release();
        assert!(surface.canvas(10, 10).is_ok());
    }
}
