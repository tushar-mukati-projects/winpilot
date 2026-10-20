use winpilot_core::Rect;

use crate::overlay::{
    color::{Rgb, alpha_byte},
    layer::Layer,
    palette::Palette,
    scene_frame::TrailFrame,
    trail::{MAX_TRAIL_DOTS, TrailDot, paint_trail_dot},
};

pub(crate) struct TrailLayers {
    layers: [Layer; MAX_TRAIL_DOTS],
    painted: [Option<u64>; MAX_TRAIL_DOTS],
}

impl TrailLayers {
    pub(crate) fn new(layers: [Layer; MAX_TRAIL_DOTS]) -> Self {
        Self { layers, painted: [None; MAX_TRAIL_DOTS] }
    }

    pub(crate) fn show(&mut self, trail: Option<&TrailFrame<'_>>, palette: &Palette) {
        for (index, (layer, painted)) in self.layers.iter_mut().zip(&mut self.painted).enumerate() {
            let dot = trail.and_then(|trail| trail.dots.get(index).map(|dot| (trail, dot)));
            let Some((trail, dot)) = dot else {
                layer.hide();
                *painted = None;
                continue;
            };
            let alpha = alpha_byte(dot.alpha(trail.elapsed));
            if alpha == 0 {
                layer.hide();
            } else if *painted == Some(trail.serial) && layer.placement().is_some() {
                layer.set_alpha(alpha);
            } else {
                let tone = palette.at(index as f32 / trail.dots.len().max(1) as f32 * 0.5);
                *painted = show_dot(layer, dot, alpha, tone).then_some(trail.serial);
            }
        }
    }

    pub(crate) fn layers(&self) -> &[Layer; MAX_TRAIL_DOTS] {
        &self.layers
    }

    pub(crate) fn layers_mut(&mut self) -> &mut [Layer; MAX_TRAIL_DOTS] {
        &mut self.layers
    }
}

fn show_dot(layer: &mut Layer, dot: &TrailDot, alpha: u8, tone: Rgb) -> bool {
    let reach = dot.reach();
    let at = Rect::new(dot.center.x - reach, dot.center.y - reach, reach * 2, reach * 2);
    layer.paint(at, alpha, |canvas| paint_trail_dot(canvas, dot, tone))
}
