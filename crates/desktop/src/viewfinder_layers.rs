use deckhand_core::{Point, Rect};

use crate::{
    edge_strips::{EDGE_COUNT, border_strips},
    frame::inflate,
    overlay_color::alpha_byte,
    overlay_frame::ViewfinderFrame,
    overlay_layer::Layer,
    palette::Palette,
    strip_layers::{StripKey, StripLayers, with_strips},
    tint_window::TintWindow,
    viewfinder::{
        BracketMetrics, CORNERS, FLASH_BORDER_ALPHA, FLASH_TINT_ALPHA, bracket_alpha, bracket_offset, bracket_window,
        flash_level, flash_width, paint_bracket, paint_flash_band,
    },
};

pub(crate) struct ViewfinderLayers {
    brackets: [Layer; 4],
    painted: Option<u32>,
    flash: StripLayers,
    tint: TintWindow,
}

impl ViewfinderLayers {
    pub(crate) fn new(brackets: [Layer; 4], flash: StripLayers, tint: TintWindow) -> Self {
        Self { brackets, painted: None, flash, tint }
    }

    pub(crate) fn show(&mut self, frame: &ViewfinderFrame, palette: &Palette, screen: Rect) {
        self.show_brackets(frame, palette);
        let level = flash_level(frame.elapsed, frame.animated);
        if level <= 0.0 {
            self.flash.hide();
            self.tint.hide();
            return;
        }
        self.show_flash(frame, level, screen);
        let tint = frame.rect.intersection(screen).unwrap_or(frame.rect);
        self.tint.show(tint, alpha_byte(FLASH_TINT_ALPHA * level));
    }

    pub(crate) fn hide(&mut self) {
        for layer in &mut self.brackets {
            layer.hide();
        }
        self.flash.hide();
        self.tint.hide();
    }

    pub(crate) fn release_memory(&mut self) {
        self.painted = None;
    }

    pub(crate) fn layers(&self) -> impl Iterator<Item = &Layer> {
        self.brackets.iter().chain(self.flash.layers())
    }

    pub(crate) fn layers_mut(&mut self) -> impl Iterator<Item = &mut Layer> {
        self.brackets.iter_mut().chain(self.flash.layers_mut().iter_mut())
    }

    fn show_brackets(&mut self, frame: &ViewfinderFrame, palette: &Palette) {
        let metrics = BracketMetrics::at_scale(frame.scale);
        let offset = bracket_offset(frame.elapsed, &metrics, frame.animated);
        let alpha = alpha_byte(bracket_alpha(frame.elapsed, frame.animated));
        let scale_bits = frame.scale.to_bits();
        let fresh = self.painted != Some(scale_bits) || self.brackets.iter().any(|layer| layer.placement().is_none());
        for (layer, corner) in self.brackets.iter_mut().zip(CORNERS) {
            let at = bracket_window(frame.rect, corner, offset, &metrics);
            if fresh {
                layer.paint(at, alpha, |canvas| paint_bracket(canvas, corner, &metrics, palette));
            } else {
                layer.place(Point::new(at.x, at.y), alpha);
            }
        }
        self.painted = Some(scale_bits);
    }

    fn show_flash(&mut self, frame: &ViewfinderFrame, level: f32, screen: Rect) {
        let width = flash_width(frame.scale);
        let strips = border_strips(inflate(frame.rect, -width), width, screen);
        let key = StripKey { strip: Rect::default(), shape: frame.rect, style: u64::from(frame.scale.to_bits()) };
        let keys = with_strips([Some(key); EDGE_COUNT], strips);
        let rect = frame.rect;
        let alpha = alpha_byte(FLASH_BORDER_ALPHA * level);
        self.flash.show(strips, keys, alpha, |canvas, origin| paint_flash_band(canvas, origin, rect, width));
    }
}
