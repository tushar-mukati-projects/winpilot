use deckhand_core::{Point, Rect};

use crate::{
    edge_strips::{EDGE_COUNT, border_strips},
    focus_outline::{FocusMetrics, RingPixel, focus_ring, paint_focus, shade_ring},
    frame::inflate,
    overlay_color::alpha_byte,
    overlay_frame::FocusFrame,
    overlay_layer::Layer,
    palette::Palette,
    strip_layers::{StripKey, StripLayers, with_strips},
};

const WHOLE_WINDOW_LIMIT: i64 = 160_000;

pub(crate) struct FocusLayers {
    strips: StripLayers,
    whole: Layer,
    painted: Option<StripKey>,
    ring: Option<(StripKey, Vec<RingPixel>)>,
}

impl FocusLayers {
    pub(crate) fn new(strips: StripLayers, whole: Layer) -> Self {
        Self { strips, whole, painted: None, ring: None }
    }

    pub(crate) fn show(&mut self, focus: &FocusFrame, palette: &Palette, repaint: bool, screen: Rect) {
        let metrics = FocusMetrics::at_scale(focus.scale);
        let core = metrics.band_core(focus.element);
        let outline = inflate(core, metrics.band()).intersection(screen);
        let alpha = alpha_byte(focus.alpha);
        let look = Look { palette, repaint: repaint || !focus.steady };
        if let Some(area) = outline.filter(|area| area.area() <= WHOLE_WINDOW_LIMIT) {
            self.strips.hide();
            self.show_whole(focus, &metrics, area, alpha, &look);
            return;
        }
        self.whole.hide();
        self.painted = None;
        if look.repaint {
            self.strips.forget();
        }
        self.show_strips(focus, &metrics, border_strips(core, metrics.band(), screen), alpha, &look);
    }

    pub(crate) fn hide(&mut self) {
        self.strips.hide();
        self.whole.hide();
        self.painted = None;
    }

    pub(crate) fn layers(&self) -> impl Iterator<Item = &Layer> {
        self.strips.layers().iter().chain([&self.whole])
    }

    pub(crate) fn layers_mut(&mut self) -> impl Iterator<Item = &mut Layer> {
        self.strips.layers_mut().iter_mut().chain([&mut self.whole])
    }

    fn show_whole(&mut self, focus: &FocusFrame, metrics: &FocusMetrics, area: Rect, alpha: u8, look: &Look<'_>) {
        let key = key_for(focus, area);
        if self.painted == Some(key) && self.whole.placement() == Some(area) && !look.repaint {
            self.whole.set_alpha(alpha);
            return;
        }
        let origin = Point::new(area.x, area.y);
        if !self.ring.as_ref().is_some_and(|(ring_key, _)| *ring_key == key) {
            let size = (area.width.max(0) as usize, area.height.max(0) as usize);
            self.ring = Some((key, focus_ring(origin, size, focus.element, metrics)));
        }
        let Some((_, ring)) = &self.ring else { return };
        let painted = self.whole.paint(area, alpha, |canvas| shade_ring(canvas, ring, look.palette, focus.look));
        self.painted = painted.then_some(key);
    }

    fn show_strips(
        &mut self,
        focus: &FocusFrame,
        metrics: &FocusMetrics,
        strips: [Option<Rect>; EDGE_COUNT],
        alpha: u8,
        look: &Look<'_>,
    ) {
        let keys = with_strips([Some(key_for(focus, Rect::default())); EDGE_COUNT], strips);
        self.strips.show(strips, keys, alpha, |canvas, origin| {
            paint_focus(canvas, origin, focus.element, metrics, (look.palette, focus.look));
        });
    }
}

struct Look<'a> {
    palette: &'a Palette,
    repaint: bool,
}

fn key_for(focus: &FocusFrame, strip: Rect) -> StripKey {
    StripKey { strip, shape: focus.element, style: u64::from(focus.scale.to_bits()) }
}
