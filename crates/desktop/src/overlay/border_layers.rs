use deckhand_core::{Point, Rect};

use super::{
    border_geometry::BorderMetrics,
    border_paint::BorderPainter,
    edge_strips::{EDGE_COUNT, border_strips},
    glow_profile::GlowProfile,
    glow_tints::{GlowLook, GlowTints},
    layer::Layer,
    palette::Palette,
    perimeter::Perimeter,
};

pub(crate) struct BorderLook<'a> {
    pub(crate) rect: Rect,
    pub(crate) metrics: BorderMetrics,
    pub(crate) palette: &'a Palette,
    pub(crate) glow: GlowLook,
    pub(crate) alpha: u8,
    pub(crate) tick: u64,
    pub(crate) steady: bool,
}

const SMALL_FRAME: i64 = 80_000;
const LARGE_FRAME: i64 = 200_000;

fn steady_cadence(pixels: i64) -> u64 {
    if pixels <= SMALL_FRAME {
        2
    } else if pixels <= LARGE_FRAME {
        3
    } else {
        4
    }
}

pub(crate) struct BorderLayers {
    layers: [Layer; EDGE_COUNT],
    placed: [Option<Rect>; EDGE_COUNT],
    painter: BorderPainter,
    tints: GlowTints,
    profile: Option<GlowProfile>,
}

impl BorderLayers {
    pub(crate) fn new(layers: [Layer; EDGE_COUNT]) -> Self {
        Self {
            layers,
            placed: [None; EDGE_COUNT],
            painter: BorderPainter::default(),
            tints: GlowTints::default(),
            profile: None,
        }
    }

    pub(crate) fn show(&mut self, look: &BorderLook<'_>, screen: Rect) {
        if !self.profile.as_ref().is_some_and(|profile| profile.fits(look.metrics)) {
            self.profile = Some(GlowProfile::new(look.metrics));
            self.placed = [None; EDGE_COUNT];
        }
        let strips = border_strips(look.rect, look.metrics.band, screen);
        let unchanged =
            self.placed == strips && self.layers.iter().zip(strips).all(|(layer, strip)| layer.placement() == strip);
        let pixels = strips.iter().flatten().map(|strip| strip.area()).sum();
        let due = !look.steady || look.tick.is_multiple_of(steady_cadence(pixels));
        if unchanged && !due {
            self.layers.iter_mut().for_each(|layer| layer.set_alpha(look.alpha));
            return;
        }
        let Some(profile) = &self.profile else { return };
        self.tints.refresh(&Perimeter::of(look.rect), look.palette, &look.glow);
        for ((layer, placed), strip) in self.layers.iter_mut().zip(&mut self.placed).zip(strips) {
            let Some(strip) = strip else {
                layer.hide();
                *placed = None;
                continue;
            };
            let (painter, tints) = (&mut self.painter, &self.tints);
            let origin = Point::new(strip.x, strip.y);
            let shown =
                layer.paint(strip, look.alpha, |canvas| painter.paint(canvas, origin, look.rect, profile, tints));
            *placed = shown.then_some(strip);
        }
    }

    pub(crate) fn hide(&mut self) {
        for layer in &mut self.layers {
            layer.hide();
        }
        self.placed = [None; EDGE_COUNT];
    }

    pub(crate) fn layers_mut(&mut self) -> &mut [Layer; EDGE_COUNT] {
        &mut self.layers
    }

    pub(crate) fn layers(&self) -> &[Layer; EDGE_COUNT] {
        &self.layers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bigger_frames_repaint_their_steady_flow_less_often() {
        assert_eq!(steady_cadence(40_000), 2);
        assert_eq!(steady_cadence(120_000), 3);
        assert_eq!(steady_cadence(221_000), 4);
    }
}
