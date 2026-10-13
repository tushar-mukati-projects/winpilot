use winpilot_core::{Error, Result};

use super::text_render::PillFonts;

#[derive(Default)]
pub(crate) struct FontCache {
    fonts: Option<PillFonts>,
}

impl FontCache {
    pub(crate) fn at(&mut self, px: i32) -> Result<&PillFonts> {
        if self.fonts.as_ref().is_none_or(|fonts| fonts.px != px) {
            self.fonts = Some(PillFonts::new(px)?);
        }
        self.fonts.as_ref().ok_or_else(|| Error::internal("the overlay fonts are missing"))
    }

    pub(crate) fn release(&mut self) {
        self.fonts = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fonts_are_made_once_per_size() {
        let mut cache = FontCache::default();
        let first = std::ptr::from_ref(cache.at(17).unwrap().lead());
        assert_eq!(std::ptr::from_ref(cache.at(17).unwrap().lead()), first);
        assert_eq!(cache.at(21).unwrap().px, 21);
        cache.release();
        assert!(cache.fonts.is_none());
    }
}
