use deckhand_core::{Point, Rect};

use crate::{
    edge_strips::{EDGE_COUNT, border_strips},
    overlay_color::alpha_byte,
    overlay_frame::ScanFrame,
    overlay_layer::Layer,
    palette::Palette,
    scan::{band_height, band_top, edge_alpha, edge_width, paint_band, paint_scan_edge},
    strip_layers::{StripKey, StripLayers, with_strips},
};

pub(crate) struct ScanLayers {
    band: Layer,
    band_key: Option<(Rect, u32)>,
    edges: StripLayers,
}

impl ScanLayers {
    pub(crate) fn new(band: Layer, edges: StripLayers) -> Self {
        Self { band, band_key: None, edges }
    }

    pub(crate) fn show(&mut self, scan: &ScanFrame, palette: &Palette, screen: Rect) {
        let width = edge_width(scan.scale);
        let strips = border_strips(scan.rect, width, screen);
        let key = StripKey { strip: Rect::default(), shape: scan.rect, style: u64::from(scan.scale.to_bits()) };
        let keys = with_strips([Some(key); EDGE_COUNT], strips);
        let (rect, scale) = (scan.rect, scan.scale);
        let alpha = alpha_byte(edge_alpha(scan.elapsed, scan.animated));
        self.edges.show(strips, keys, alpha, |canvas, origin| paint_scan_edge(canvas, origin, rect, palette, scale));
        if scan.animated {
            self.show_band(scan, palette, screen);
        } else {
            self.band.hide();
        }
    }

    pub(crate) fn hide(&mut self) {
        self.band.hide();
        self.edges.hide();
    }

    pub(crate) fn layers(&self) -> impl Iterator<Item = &Layer> {
        self.edges.layers().iter().chain([&self.band])
    }

    pub(crate) fn layers_mut(&mut self) -> impl Iterator<Item = &mut Layer> {
        self.edges.layers_mut().iter_mut().chain([&mut self.band])
    }

    fn show_band(&mut self, scan: &ScanFrame, palette: &Palette, screen: Rect) {
        let Some(visible) = scan.rect.intersection(screen) else {
            self.band.hide();
            return;
        };
        let top = band_top(scan.rect, scan.elapsed, scan.scale);
        let placement = Rect::new(visible.x, top, visible.width, band_height(scan.scale));
        let key = (visible, scan.scale.to_bits());
        if self.band_key == Some(key) && self.band.placement().is_some() {
            self.band.place(Point::new(placement.x, placement.y), u8::MAX);
            return;
        }
        let painted = self.band.paint(placement, u8::MAX, |canvas| paint_band(canvas, palette, 0.0));
        self.band_key = painted.then_some(key);
    }
}
