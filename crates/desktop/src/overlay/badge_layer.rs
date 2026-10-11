use deckhand_core::{Error, Point, Result};

use super::{
    color::alpha_byte,
    cursor_badge::{BadgeLayout, BadgeMetrics, layout_badge, paint_badge},
    font_cache::FontCache,
    layer::Layer,
    palette::Palette,
    scene_frame::BadgeFrame,
    text_mask::TextMask,
    text_render::render_text,
};

struct Art {
    scale_bits: u32,
    layout: BadgeLayout,
}

pub(crate) struct BadgeLayer {
    layer: Layer,
    label: String,
    fonts: FontCache,
    art: Option<Art>,
}

impl BadgeLayer {
    pub(crate) fn new(layer: Layer, label: String) -> Self {
        Self { layer, label, fonts: FontCache::default(), art: None }
    }

    pub(crate) fn show(&mut self, badge: &BadgeFrame, palette: &Palette) {
        let alpha = alpha_byte(badge.alpha);
        let fresh = self.art.as_ref().is_none_or(|art| art.scale_bits != badge.scale.to_bits());
        if fresh || self.layer.placement().is_none() {
            if self.paint(badge, palette, alpha).is_err() {
                self.layer.hide();
            }
            return;
        }
        if let Some(art) = &self.art {
            let window = art.layout.window_at(badge.at);
            self.layer.place(Point::new(window.x, window.y), alpha);
        }
    }

    pub(crate) fn hide(&mut self) {
        self.layer.hide();
    }

    pub(crate) fn release_memory(&mut self) {
        self.art = None;
        self.fonts.release();
        self.layer.release_memory();
    }

    pub(crate) fn layer(&self) -> &Layer {
        &self.layer
    }

    pub(crate) fn layer_mut(&mut self) -> &mut Layer {
        &mut self.layer
    }

    fn paint(&mut self, badge: &BadgeFrame, palette: &Palette, alpha: u8) -> Result<()> {
        let metrics = BadgeMetrics::at_scale(badge.scale);
        let fonts = self.fonts.at(metrics.font_px)?;
        let label: TextMask = render_text(&self.label, fonts.lead())?;
        let size = (i32::try_from(label.width).unwrap_or(0), i32::try_from(label.height).unwrap_or(0));
        let layout = layout_badge(&metrics, size);
        let window = layout.window_at(badge.at);
        if !self.layer.paint(window, alpha, |canvas| paint_badge(canvas, &layout, &metrics, palette, &label)) {
            return Err(Error::internal("showing the cursor badge"));
        }
        self.art = Some(Art { scale_bits: badge.scale.to_bits(), layout });
        Ok(())
    }
}
