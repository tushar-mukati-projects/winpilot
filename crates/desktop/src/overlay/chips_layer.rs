use deckhand_core::{Error, Result};

use super::{
    color::alpha_byte,
    font_cache::FontCache,
    keycaps::{CapMetrics, Item, layout_caps, paint_caps},
    layer::Layer,
    palette::Palette,
    scene_frame::ChipsFrame,
    text_mask::TextMask,
    text_render::render_text,
};

pub(crate) struct ChipsLayer {
    layer: Layer,
    fonts: FontCache,
    painted: Option<u64>,
}

impl ChipsLayer {
    pub(crate) fn new(layer: Layer) -> Self {
        Self { layer, fonts: FontCache::default(), painted: None }
    }

    pub(crate) fn show(&mut self, chips: &ChipsFrame<'_>, palette: &Palette) {
        let alpha = alpha_byte(chips.alpha);
        if self.painted == Some(chips.serial) && self.layer.placement().is_some() {
            self.layer.set_alpha(alpha);
            return;
        }
        self.painted = self.paint(chips, palette, alpha).ok().map(|()| chips.serial);
        if self.painted.is_none() {
            self.layer.hide();
        }
    }

    pub(crate) fn hide(&mut self) {
        self.layer.hide();
        self.painted = None;
    }

    pub(crate) fn release_memory(&mut self) {
        self.fonts.release();
        self.painted = None;
        self.layer.release_memory();
    }

    pub(crate) fn layer(&self) -> &Layer {
        &self.layer
    }

    pub(crate) fn layer_mut(&mut self) -> &mut Layer {
        &mut self.layer
    }

    fn paint(&mut self, chips: &ChipsFrame<'_>, palette: &Palette, alpha: u8) -> Result<()> {
        let metrics = CapMetrics::at_scale(chips.scale);
        let fonts = self.fonts.at(metrics.font_px)?;
        let items = chips.text.items();
        let masks: Vec<TextMask> = items
            .iter()
            .filter_map(|item| match item {
                Item::Cap(text) => Some(render_text(text, fonts.lead())),
                Item::Plus | Item::Gap => None,
            })
            .collect::<Result<_>>()?;
        let plus = render_text("+", fonts.hint())?;
        let layout = layout_caps(&items, &masks, &plus, &metrics);
        let window = layout.window_in(chips.rect, &metrics);
        let painted = self.layer.paint(window, alpha, |canvas| paint_caps(canvas, &layout, &masks, &plus, palette));
        if painted { Ok(()) } else { Err(Error::internal("showing the keystroke chips")) }
    }
}
