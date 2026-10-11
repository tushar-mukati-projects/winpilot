use deckhand_core::{Point, Rect, Result};
use windows::Win32::Foundation::HINSTANCE;

use crate::{canvas::Canvas, edge_strips::EDGE_COUNT, overlay_layer::Layer, overlay_window::Capture};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct StripKey {
    pub(crate) strip: Rect,
    pub(crate) shape: Rect,
    pub(crate) style: u64,
}

pub(crate) struct StripLayers {
    layers: [Layer; EDGE_COUNT],
    painted: [Option<StripKey>; EDGE_COUNT],
}

impl StripLayers {
    pub(crate) fn create(instance: HINSTANCE, capture: Capture) -> Result<(Self, bool)> {
        let mut applied = true;
        let mut make = || -> Result<Layer> {
            let (layer, excluded) = Layer::create(instance, capture)?;
            applied &= excluded;
            Ok(layer)
        };
        let layers = [make()?, make()?, make()?, make()?];
        Ok((Self { layers, painted: [None; EDGE_COUNT] }, applied))
    }

    pub(crate) fn show(
        &mut self,
        strips: [Option<Rect>; EDGE_COUNT],
        keys: [Option<StripKey>; EDGE_COUNT],
        alpha: u8,
        mut paint: impl FnMut(&mut Canvas<'_>, Point),
    ) {
        for (index, (strip, key)) in strips.into_iter().zip(keys).enumerate() {
            let (Some(layer), Some(painted)) = (self.layers.get_mut(index), self.painted.get_mut(index)) else {
                continue;
            };
            let Some(strip) = strip else {
                layer.hide();
                *painted = None;
                continue;
            };
            if *painted == key && layer.placement() == Some(strip) {
                layer.set_alpha(alpha);
                continue;
            }
            let origin = Point::new(strip.x, strip.y);
            *painted = layer.paint(strip, alpha, |canvas| paint(canvas, origin)).then_some(key).flatten();
        }
    }

    pub(crate) fn hide(&mut self) {
        for layer in &mut self.layers {
            layer.hide();
        }
        self.forget();
    }

    pub(crate) fn forget(&mut self) {
        self.painted = [None; EDGE_COUNT];
    }

    pub(crate) fn layers_mut(&mut self) -> &mut [Layer; EDGE_COUNT] {
        &mut self.layers
    }

    pub(crate) fn layers(&self) -> &[Layer; EDGE_COUNT] {
        &self.layers
    }
}

pub(crate) fn with_strips(
    keys: [Option<StripKey>; EDGE_COUNT],
    strips: [Option<Rect>; EDGE_COUNT],
) -> [Option<StripKey>; EDGE_COUNT] {
    let mut paired = keys;
    for (key, strip) in paired.iter_mut().zip(strips) {
        *key = key.zip(strip).map(|(key, strip)| StripKey { strip, ..key });
    }
    paired
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_carry_their_strip() {
        let key = StripKey { strip: Rect::default(), shape: Rect::new(0, 0, 10, 10), style: 3 };
        let strips = [Some(Rect::new(1, 2, 3, 4)), None, Some(Rect::new(5, 6, 7, 8)), None];
        let paired = with_strips([Some(key); EDGE_COUNT], strips);
        assert_eq!(paired[0].map(|key| key.strip), Some(Rect::new(1, 2, 3, 4)));
        assert_eq!(paired[1], None);
        assert_eq!(paired[2].map(|key| (key.strip, key.style)), Some((Rect::new(5, 6, 7, 8), 3)));
    }
}
