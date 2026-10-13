use std::f32::consts::TAU;

use super::{
    color::{Rgb, WHITE},
    easing::{ease_out_cubic, smoothstep},
    palette::Palette,
    perimeter::{Perimeter, wrapped_distance},
};

const STEP: f32 = 6.0;
const CORE_WHITENESS: f32 = 0.35;
const MAX_GAIN: f32 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GlowLook {
    pub(crate) flow: f32,
    pub(crate) shimmer: f32,
    pub(crate) ignite: Option<f32>,
    pub(crate) gain: f32,
}

impl GlowLook {
    pub(crate) const STILL: Self = Self { flow: 0.0, shimmer: 0.25, ignite: None, gain: 1.0 };
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Tint {
    core: [u32; 3],
    bloom: [u32; 3],
    gain: u32,
}

impl Tint {
    pub(crate) fn of(core: Rgb, bloom: Rgb, gain: f32) -> Self {
        let gain = (gain.clamp(0.0, MAX_GAIN) * 256.0).round() as u32;
        let scaled = |color: Rgb| [color.red, color.green, color.blue].map(|channel| u32::from(channel) * gain);
        Self { core: scaled(core), bloom: scaled(bloom), gain }
    }

    #[cfg(test)]
    pub(crate) fn gain(&self) -> u32 {
        self.gain
    }

    #[inline]
    pub(crate) fn pixel(&self, core_alpha: u16, bloom_alpha: u16) -> u32 {
        let (core, bloom) = (u32::from(core_alpha), u32::from(bloom_alpha));
        let alpha = (((core + bloom) * self.gain) >> 8).min(255);
        let red = ((self.core[0] * core + self.bloom[0] * bloom) >> 16).min(alpha);
        let green = ((self.core[1] * core + self.bloom[1] * bloom) >> 16).min(alpha);
        let blue = ((self.core[2] * core + self.bloom[2] * bloom) >> 16).min(alpha);
        (alpha << 24) | (red << 16) | (green << 8) | blue
    }
}

#[derive(Debug, Default)]
pub(crate) struct GlowTints {
    tints: Vec<Tint>,
}

impl GlowTints {
    pub(crate) fn refresh(&mut self, perimeter: &Perimeter, palette: &Palette, look: &GlowLook) {
        let count = (perimeter.length() / STEP).ceil() as usize + 1;
        self.tints.clear();
        self.tints.extend((0..count).map(|index| tint_at(index as f32 * STEP, perimeter, palette, look)));
    }

    #[cfg(test)]
    pub(crate) fn at(&self, along: f32) -> Tint {
        self.tint(self.index_at(along))
    }

    pub(crate) fn index_at(&self, along: f32) -> u32 {
        let last = self.tints.len().saturating_sub(1);
        ((along.max(0.0) / STEP).round() as usize).min(last) as u32
    }

    pub(crate) fn tint(&self, index: u32) -> Tint {
        self.tints.get(index as usize).copied().unwrap_or_default()
    }
}

fn tint_at(along: f32, perimeter: &Perimeter, palette: &Palette, look: &GlowLook) -> Tint {
    let position = along / perimeter.length().max(1.0);
    let color = palette.at(position - look.flow);
    let shimmer = 0.84 + 0.26 * (0.5 + 0.5 * (TAU * (2.0 * position - look.shimmer)).sin());
    let ignite = look.ignite.map_or(1.0, |progress| ignite_gain(along, perimeter, progress));
    Tint::of(color.mix(WHITE, CORE_WHITENESS), color, shimmer * ignite * look.gain)
}

fn ignite_gain(along: f32, perimeter: &Perimeter, progress: f32) -> f32 {
    let half = (perimeter.length() / 2.0).max(1.0);
    let from_top = wrapped_distance(perimeter.width() / 2.0, along, perimeter.length()).abs() / half;
    let front = ease_out_cubic(progress) * 1.1;
    let lit = 1.0 - smoothstep(front - 0.08, front, from_top);
    let flare = 1.2 * (-((from_top - front) / 0.04).powi(2)).exp() * (1.0 - progress);
    lit + flare
}

#[cfg(test)]
mod tests {
    use winpilot_core::Rect;

    use super::*;

    fn perimeter() -> Perimeter {
        Perimeter::of(Rect::new(0, 0, 400, 200))
    }

    #[test]
    fn pixels_are_premultiplied_and_saturate_at_full_alpha() {
        let tint = Tint::of(Rgb::new(255, 255, 255), Rgb::new(124, 58, 237), 4.0);
        assert_eq!(tint.pixel(255, 0) >> 24, 255);
        for core in [0u16, 40, 255] {
            for bloom in [0u16, 90, 255] {
                let pixel = tint.pixel(core, bloom);
                assert!((0..3).all(|channel| (pixel >> (channel * 8)) & 0xFF <= pixel >> 24), "{pixel:08x}");
            }
        }
        assert_eq!(Tint::default().pixel(255, 255), 0);
    }

    #[test]
    fn colours_flow_along_the_perimeter_over_time() {
        let palette = Palette::aurora();
        let mut tints = GlowTints::default();
        let still = GlowLook { flow: 0.0, shimmer: 0.0, ..GlowLook::STILL };
        tints.refresh(&perimeter(), &palette, &still);
        let start = tints.at(0.0);
        let later = GlowLook { flow: 0.25, shimmer: 0.0, ..GlowLook::STILL };
        tints.refresh(&perimeter(), &palette, &later);
        assert_ne!(tints.at(0.0).bloom, start.bloom);
        assert_eq!(tints.at(300.0).bloom, start.bloom);
    }

    #[test]
    fn ignition_spreads_both_ways_from_the_top_centre() {
        let frame = perimeter();
        let top_centre = 200.0;
        assert!(ignite_gain(top_centre, &frame, 0.1) > 0.9);
        assert!(ignite_gain(top_centre + 600.0, &frame, 0.1) < 0.05);
        assert!(
            (ignite_gain(top_centre + 150.0, &frame, 0.5) - ignite_gain(top_centre - 150.0, &frame, 0.5)).abs() < 1e-4
        );
        assert!((ignite_gain(top_centre + 600.0, &frame, 1.0) - 1.0).abs() < 0.01);
    }

    #[test]
    fn shimmer_keeps_the_glow_within_a_gentle_range() {
        let mut tints = GlowTints::default();
        tints.refresh(&perimeter(), &Palette::aurora(), &GlowLook::STILL);
        let gains: Vec<u32> = (0..1200).step_by(7).map(|along| tints.at(along as f32).gain()).collect();
        assert!(gains.iter().all(|gain| (210..=290).contains(gain)), "{gains:?}");
    }
}
