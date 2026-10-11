use deckhand_core::{Error, Point, Rect, Result};

use super::{
    canvas::Bitmap,
    color::{WHITE, alpha_byte},
    easing::{ease_out_cubic, progress_of},
    font_cache::FontCache,
    layer::Layer,
    palette::Palette,
    pill_kind::{Glyph, PillKind},
    pill_label::Speaker,
    pill_layout::{PillLayout, PillMetrics, layout_pill, spring},
    pill_paint::{PillArt, RimPixel, paint_dot, paint_pill, paint_rim, paint_sparkle, pill_rim, rest_rim_on},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PillShow {
    pub(crate) kind: PillKind,
    pub(crate) anchor: Rect,
    pub(crate) scale: f32,
    pub(crate) alpha: f32,
    pub(crate) pulse: f32,
    pub(crate) spin: f32,
    pub(crate) entrance: f32,
    pub(crate) repaint: bool,
}

struct Art {
    kind: PillKind,
    scale_bits: u32,
    metrics: PillMetrics,
    layout: PillLayout,
    base: Bitmap,
    rim: Vec<RimPixel>,
}

pub(crate) struct PillVoice {
    pub(crate) label: String,
    pub(crate) hotkey: Option<String>,
    pub(crate) palette: Palette,
    pub(crate) alarm: Palette,
}

pub(crate) struct PillLayer {
    layer: Layer,
    voice: PillVoice,
    fonts: FontCache,
    art: Option<Art>,
}

impl PillLayer {
    pub(crate) fn new(layer: Layer, voice: PillVoice) -> Self {
        Self { layer, voice, fonts: FontCache::default(), art: None }
    }

    pub(crate) fn show(&mut self, request: &PillShow, bounds: Rect) {
        let fresh =
            self.art.as_ref().is_none_or(|art| art.kind != request.kind || art.scale_bits != request.scale.to_bits());
        if fresh && self.prepare(request, bounds).is_err() {
            self.hide();
            return;
        }
        let Some(art) = &self.art else { return };
        let settled = spring(request.entrance);
        let lift = (art.metrics.slide as f32 * (1.0 - settled)).round() as i32;
        let layout = art.layout.moved_to(request.anchor, &art.metrics, request.kind.align(), bounds).lifted(lift);
        let appear = ease_out_cubic(progress_of(request.entrance, 0.45));
        let alpha = alpha_byte(request.alpha * appear);
        let resized = self.layer.placement().is_none_or(|shown| shown.width != layout.window.width);
        if !(fresh || request.repaint || resized) {
            self.layer.place(Point::new(layout.window.x, layout.window.y), alpha);
            return;
        }
        let palette = if request.kind.alarming() { &self.voice.alarm } else { &self.voice.palette };
        let whole = Rect::new(0, 0, layout.window.width, layout.window.height);
        let restore = if fresh || resized { whole } else { layout.glyph_region() };
        self.layer.paint(layout.window, alpha, |canvas| {
            canvas.copy_from(&art.base, restore);
            paint_rim(canvas, &art.rim, palette, request.spin);
            match request.kind.glyph() {
                Glyph::Sparkle => paint_sparkle(canvas, &layout, palette, request.spin, request.pulse),
                Glyph::Dot => paint_dot(canvas, &layout, WHITE, request.pulse),
            }
        });
    }

    pub(crate) fn hide(&mut self) {
        self.layer.hide();
    }

    pub(crate) fn relabel(&mut self, label: &str) {
        label.clone_into(&mut self.voice.label);
        self.art = None;
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

    fn prepare(&mut self, request: &PillShow, bounds: Rect) -> Result<()> {
        let metrics = PillMetrics::at_scale(request.scale);
        let fonts = self.fonts.at(metrics.font_px)?;
        let speaker = Speaker { label: &self.voice.label, hotkey: self.voice.hotkey.as_deref() };
        let label = request.kind.label(fonts, &metrics, &speaker)?;
        let size = (label.width(), label.height());
        let layout = layout_pill(request.anchor, &metrics, request.kind.align(), size, bounds);
        let mut base = Bitmap::new(layout.window.width.max(0) as usize, layout.window.height.max(0) as usize);
        let art = PillArt { layout, style: request.kind.style(), lead: &label.lead, hint: label.hint.as_ref() };
        paint_pill(&mut base.canvas().ok_or_else(|| Error::internal("the pill bitmap is the wrong size"))?, &art);
        let mut rim = pill_rim(&layout);
        rest_rim_on(&mut rim, &base);
        self.art = Some(Art { kind: request.kind, scale_bits: request.scale.to_bits(), metrics, layout, base, rim });
        Ok(())
    }
}
