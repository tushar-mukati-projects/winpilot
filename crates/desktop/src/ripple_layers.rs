use deckhand_core::Rect;

use crate::{
    overlay_frame::{MAX_RIPPLES, RippleFrame},
    overlay_layer::Layer,
    palette::Palette,
    ripple::{Burst, paint_ripple, paint_still_ripple, ripple_reach},
};

pub(crate) struct RippleLayers {
    layers: [Layer; MAX_RIPPLES],
    still: [Option<u64>; MAX_RIPPLES],
}

impl RippleLayers {
    pub(crate) fn new(layers: [Layer; MAX_RIPPLES]) -> Self {
        Self { layers, still: [None; MAX_RIPPLES] }
    }

    pub(crate) fn show(&mut self, ripples: &[Option<RippleFrame>; MAX_RIPPLES], palette: &Palette) {
        for ((layer, still), ripple) in self.layers.iter_mut().zip(&mut self.still).zip(ripples) {
            match ripple {
                Some(ripple) if ripple.animated => {
                    *still = None;
                    show_moving(layer, ripple, palette);
                }
                Some(ripple) if *still != Some(ripple.serial) => {
                    *still = show_still(layer, ripple, palette).then_some(ripple.serial);
                }
                Some(_) => {}
                None => {
                    *still = None;
                    layer.hide();
                }
            }
        }
    }

    pub(crate) fn layers(&self) -> &[Layer; MAX_RIPPLES] {
        &self.layers
    }

    pub(crate) fn layers_mut(&mut self) -> &mut [Layer; MAX_RIPPLES] {
        &mut self.layers
    }
}

fn placement(ripple: &RippleFrame) -> (Rect, (f32, f32)) {
    let reach = ripple_reach(ripple.scale);
    let at = Rect::new(ripple.center.x - reach, ripple.center.y - reach, reach * 2, reach * 2);
    (at, (reach as f32, reach as f32))
}

fn show_moving(layer: &mut Layer, ripple: &RippleFrame, palette: &Palette) -> bool {
    let (at, center) = placement(ripple);
    let burst = Burst { elapsed: ripple.elapsed, bursts: ripple.rings, palette, scale: ripple.scale };
    layer.paint(at, 255, |canvas| paint_ripple(canvas, center, &burst))
}

fn show_still(layer: &mut Layer, ripple: &RippleFrame, palette: &Palette) -> bool {
    let (at, center) = placement(ripple);
    layer.paint(at, 255, |canvas| paint_still_ripple(canvas, center, palette, ripple.scale))
}
