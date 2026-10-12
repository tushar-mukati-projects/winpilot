use deckhand_core::{Error, Rect, Result};

use crate::frame::overlap;

const MAX_SIDE: i32 = 32_768;
const MAX_PIXELS: i64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CaptureArea(Rect);

impl CaptureArea {
    pub(crate) fn new(rect: Rect) -> Result<Self> {
        if rect.width <= 0 || rect.height <= 0 {
            return Err(Error::invalid("capture rectangle is empty"));
        }
        if rect.width > MAX_SIDE || rect.height > MAX_SIDE {
            return Err(Error::invalid(format!("capture rectangle sides must be at most {MAX_SIDE} px")));
        }
        if i64::from(rect.width) * i64::from(rect.height) > MAX_PIXELS {
            return Err(Error::invalid(format!("capture rectangle must hold at most {MAX_PIXELS} pixels")));
        }
        if rect.x.checked_add(rect.width).is_none() || rect.y.checked_add(rect.height).is_none() {
            return Err(Error::invalid("capture rectangle extends past the coordinate range"));
        }
        Ok(Self(rect))
    }

    pub(crate) fn clip(self, rect: Rect) -> Option<Self> {
        overlap(self.0, rect).map(Self)
    }

    pub(crate) const fn rect(self) -> Rect {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use deckhand_core::ErrorCode;

    use super::*;

    #[test]
    fn valid_rects_are_accepted() {
        let area = CaptureArea::new(Rect::new(-100, -100, 200, 100)).unwrap();
        assert_eq!(area.rect(), Rect::new(-100, -100, 200, 100));
    }

    #[test]
    fn invalid_rects_are_rejected() {
        let failures = [
            Rect::new(0, 0, 0, 10),
            Rect::new(0, 0, 10, -1),
            Rect::new(0, 0, MAX_SIDE + 1, 10),
            Rect::new(0, 0, MAX_SIDE, MAX_SIDE),
            Rect::new(i32::MAX - 5, 0, 10, 10),
        ];
        for rect in failures {
            assert_eq!(CaptureArea::new(rect).unwrap_err().code, ErrorCode::InvalidArgument, "{rect:?}");
        }
    }

    #[test]
    fn clipping_stays_inside_the_validated_area() {
        let area = CaptureArea::new(Rect::new(0, 0, 100, 50)).unwrap();
        assert_eq!(area.clip(Rect::new(90, 40, 30, 30)).map(CaptureArea::rect), Some(Rect::new(90, 40, 10, 10)));
        assert_eq!(area.clip(Rect::new(100, 0, 5, 5)), None);
    }
}
