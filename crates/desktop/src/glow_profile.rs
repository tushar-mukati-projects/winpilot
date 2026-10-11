use crate::{border_geometry::BorderMetrics, easing::smoothstep};

const SUBSTEPS: f32 = 4.0;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GlowProfile {
    metrics: BorderMetrics,
    reach: f32,
    core: Vec<u16>,
    bloom: Vec<u16>,
}

impl GlowProfile {
    pub(crate) fn new(metrics: BorderMetrics) -> Self {
        let reach = metrics.band.max(0) as f32;
        let steps = (reach * SUBSTEPS) as usize + 1;
        let distances = (0..steps).map(|step| step as f32 / SUBSTEPS);
        let core: Vec<u16> = distances.clone().map(|distance| to_level(core_alpha(&metrics, distance))).collect();
        let bloom = distances
            .zip(&core)
            .map(|(distance, core)| to_level(bloom_alpha(&metrics, distance)) * (255 - core) / 255)
            .collect();
        Self { metrics, reach, core, bloom }
    }

    pub(crate) fn fits(&self, metrics: BorderMetrics) -> bool {
        self.metrics == metrics
    }

    pub(crate) fn at(&self, distance: f32) -> (u16, u16) {
        if !(0.0..self.reach).contains(&distance) {
            return (0, 0);
        }
        let index = (distance * SUBSTEPS).round() as usize;
        (self.core.get(index).copied().unwrap_or(0), self.bloom.get(index).copied().unwrap_or(0))
    }
}

fn core_alpha(metrics: &BorderMetrics, distance: f32) -> f32 {
    (metrics.core + 0.5 - distance).clamp(0.0, 1.0)
}

fn bloom_alpha(metrics: &BorderMetrics, distance: f32) -> f32 {
    let tight = 0.55 * (-(distance / metrics.tight_spread()).powi(2)).exp();
    let wide = 0.42 * (-(distance / metrics.wide_spread()).powi(2)).exp();
    let tail = 1.0 - smoothstep(metrics.band as f32 - 3.0, metrics.band as f32, distance);
    ((tight + wide) * tail).min(1.0)
}

fn to_level(alpha: f32) -> u16 {
    (alpha.clamp(0.0, 1.0) * 255.0).round() as u16
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> GlowProfile {
        GlowProfile::new(BorderMetrics::at_scale(1.0))
    }

    #[test]
    fn the_core_is_crisp_and_the_bloom_falls_off_smoothly() {
        let profile = profile();
        assert_eq!(profile.at(0.5).0, 255);
        assert_eq!(profile.at(1.5).0, 255);
        assert_eq!(profile.at(3.0).0, 0);
        let blooms: Vec<u16> = (2..24).map(|depth| profile.at(depth as f32 + 0.5).1).collect();
        assert!(blooms.windows(2).all(|pair| pair[1] <= pair[0]));
        assert!(blooms[0] > 80 && blooms[10] > 15 && blooms[21] < 4);
        assert_eq!(profile.at(0.5).1, 0);
    }

    #[test]
    fn nothing_is_drawn_beyond_the_band() {
        let profile = profile();
        let band = BorderMetrics::at_scale(1.0).band as f32;
        assert_eq!(profile.at(band), (0, 0));
        assert_eq!(profile.at(-0.5), (0, 0));
    }
}
