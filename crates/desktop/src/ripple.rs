use std::{f32::consts::TAU, time::Duration};

use crate::{
    canvas::Canvas,
    easing::{ease_out_cubic, lerp, progress},
    overlay_color::Premul,
    overlay_palette::WHITE,
    overlay_scale::scaled,
    palette::Palette,
    rounded_box::coverage,
};

pub(crate) const BURST_LIFE: Duration = Duration::from_millis(420);
pub(crate) const SECOND_BURST_DELAY: Duration = Duration::from_millis(90);
pub(crate) const STILL_LIFE: Duration = Duration::from_millis(320);
const INNER_RING_DELAY: Duration = Duration::from_millis(45);
const DOT_LIFE: Duration = Duration::from_millis(120);
const DOT_SOLID: Duration = Duration::from_millis(90);
const START_RADIUS: f32 = 4.0;
const END_RADIUS: f32 = 30.0;
const START_STROKE: f32 = 3.5;
const END_STROKE: f32 = 1.2;
const PARTICLES: usize = 7;
const PARTICLE_START: f32 = 6.0;
const PARTICLE_TRAVEL: f32 = 34.0;
const PARTICLE_RADIUS: f32 = 2.4;
const PEAK_ALPHA: f32 = 0.95;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Ring {
    pub(crate) radius: f32,
    pub(crate) stroke: f32,
    pub(crate) alpha: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Particle {
    pub(crate) offset: (f32, f32),
    pub(crate) radius: f32,
    pub(crate) alpha: f32,
    pub(crate) hue: f32,
}

pub(crate) fn ring_at(elapsed: Duration, scale: f32) -> Option<Ring> {
    if elapsed >= BURST_LIFE {
        return None;
    }
    let done = progress(elapsed, BURST_LIFE);
    let eased = ease_out_cubic(done);
    Some(Ring {
        radius: scaled(lerp(START_RADIUS, END_RADIUS, eased), scale),
        stroke: scaled(lerp(START_STROKE, END_STROKE, eased), scale),
        alpha: PEAK_ALPHA * (1.0 - done).powf(1.6),
    })
}

pub(crate) fn particles_at(elapsed: Duration, burst: usize, scale: f32) -> Vec<Particle> {
    if elapsed >= BURST_LIFE {
        return Vec::new();
    }
    let done = progress(elapsed, BURST_LIFE);
    let travel = scaled(PARTICLE_START + PARTICLE_TRAVEL * ease_out_cubic(done), scale);
    let twist = burst as f32 * 0.5 / PARTICLES as f32;
    (0..PARTICLES)
        .map(|index| {
            let turn = index as f32 / PARTICLES as f32 + twist;
            let angle = turn * TAU;
            Particle {
                offset: (angle.cos() * travel, angle.sin() * travel),
                radius: scaled(PARTICLE_RADIUS * (1.0 - 0.55 * done), scale),
                alpha: (1.0 - done).powf(1.3),
                hue: turn,
            }
        })
        .collect()
}

pub(crate) fn dot_alpha(elapsed: Duration) -> f32 {
    if elapsed >= DOT_LIFE {
        return 0.0;
    }
    1.0 - progress(elapsed.saturating_sub(DOT_SOLID), DOT_LIFE.saturating_sub(DOT_SOLID))
}

pub(crate) fn ripple_life(bursts: u8) -> Duration {
    BURST_LIFE + SECOND_BURST_DELAY * u32::from(bursts.clamp(1, 2) - 1)
}

pub(crate) fn ripple_reach(scale: f32) -> i32 {
    scaled(PARTICLE_START + PARTICLE_TRAVEL + PARTICLE_RADIUS + 3.0, scale).ceil() as i32
}

pub(crate) struct Burst<'a> {
    pub(crate) elapsed: Duration,
    pub(crate) bursts: u8,
    pub(crate) palette: &'a Palette,
    pub(crate) scale: f32,
}

pub(crate) fn paint_ripple(canvas: &mut Canvas<'_>, center: (f32, f32), burst: &Burst<'_>) {
    let starts: Vec<(usize, Duration)> = (0..burst.bursts.clamp(1, 2))
        .filter_map(|index| {
            burst.elapsed.checked_sub(SECOND_BURST_DELAY * u32::from(index)).map(|age| (usize::from(index), age))
        })
        .collect();
    let rings: Vec<(Ring, f32)> = starts
        .iter()
        .flat_map(|(index, age)| {
            let outer = ring_at(*age, burst.scale).map(|ring| (ring, *index as f32 * 0.37));
            let inner = age.checked_sub(INNER_RING_DELAY).and_then(|late| ring_at(late, burst.scale * 0.62));
            outer.into_iter().chain(inner.map(|ring| (ring, *index as f32 * 0.37 + 0.5)))
        })
        .collect();
    let dot = scaled(3.5, burst.scale);
    let dot_alpha = dot_alpha(burst.elapsed);
    paint_rings(canvas, center, &rings, burst.palette, (dot, dot_alpha));
    for (index, age) in starts {
        for particle in particles_at(age, index, burst.scale) {
            stamp(canvas, (center.0 + particle.offset.0, center.1 + particle.offset.1), &particle, burst.palette);
        }
    }
}

fn paint_rings(canvas: &mut Canvas<'_>, center: (f32, f32), rings: &[(Ring, f32)], palette: &Palette, dot: (f32, f32)) {
    for y in 0..canvas.height() {
        for x in 0..canvas.width() {
            let (dx, dy) = (x as f32 + 0.5 - center.0, y as f32 + 0.5 - center.1);
            let distance = dx.hypot(dy);
            let mut color = Premul::of(WHITE.mix(palette.at(0.0), 0.3), dot.1 * coverage(distance - dot.0));
            for (ring, hue) in rings {
                let off_line = (distance - ring.radius).abs() - ring.stroke / 2.0;
                if off_line > ring.stroke + 6.0 {
                    continue;
                }
                let tone = palette.at(dy.atan2(dx) / TAU + hue);
                let line = Premul::of(tone.mix(WHITE, 0.25), ring.alpha * coverage(off_line));
                let bloom =
                    Premul::of(tone, ring.alpha * 0.45 * (-(off_line.max(0.0) / (ring.stroke + 2.0)).powi(2)).exp());
                color = color.over(line.over(bloom));
            }
            canvas.set(x, y, color.to_bgra());
        }
    }
}

fn stamp(canvas: &mut Canvas<'_>, at: (f32, f32), particle: &Particle, palette: &Palette) {
    let reach = particle.radius * 2.5 + 1.0;
    let (left, top) = ((at.0 - reach).floor().max(0.0) as usize, (at.1 - reach).floor().max(0.0) as usize);
    let (right, bottom) = ((at.0 + reach).ceil().max(0.0) as usize, (at.1 + reach).ceil().max(0.0) as usize);
    let tone = palette.at(particle.hue);
    for y in top..bottom.min(canvas.height()) {
        for x in left..right.min(canvas.width()) {
            let distance = (x as f32 + 0.5 - at.0).hypot(y as f32 + 0.5 - at.1);
            let body = coverage(distance - particle.radius);
            let glow = 0.5 * (-(distance / (particle.radius * 1.8)).powi(2)).exp();
            let color = Premul::of(tone.mix(WHITE, 0.35), body).over(Premul::of(tone, glow));
            canvas.blend(x, y, color.scaled(particle.alpha));
        }
    }
}

pub(crate) fn paint_still_ripple(canvas: &mut Canvas<'_>, center: (f32, f32), palette: &Palette, scale: f32) {
    let ring = Ring { radius: scaled(11.0, scale), stroke: scaled(2.0, scale), alpha: 0.85 };
    paint_rings(canvas, center, &[(ring, 0.0)], palette, (scaled(4.5, scale), 1.0));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Bitmap;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    fn painted(elapsed: u64, bursts: u8) -> (Bitmap, i32) {
        let reach = ripple_reach(1.0);
        let side = (reach * 2) as usize;
        let mut bitmap = Bitmap::new(side, side);
        let palette = Palette::aurora();
        let burst = Burst { elapsed: MS(elapsed), bursts, palette: &palette, scale: 1.0 };
        paint_ripple(&mut bitmap.canvas().unwrap(), (reach as f32, reach as f32), &burst);
        (bitmap, reach)
    }

    #[test]
    fn the_ring_grows_from_four_to_thirty_pixels_and_thins() {
        let start = ring_at(Duration::ZERO, 1.0).unwrap();
        assert_eq!((start.radius, start.stroke, start.alpha), (4.0, 3.5, PEAK_ALPHA));
        let late = ring_at(MS(419), 1.0).unwrap();
        assert!(late.radius > 29.9 && late.stroke < 1.21);
        assert_eq!(ring_at(MS(420), 1.0), None);
    }

    #[test]
    fn radius_rises_and_alpha_falls_over_time() {
        let samples: Vec<Ring> = (0..42).filter_map(|step| ring_at(MS(step * 10), 1.0)).collect();
        assert!(samples.windows(2).all(|pair| pair[1].radius > pair[0].radius && pair[1].alpha < pair[0].alpha));
    }

    #[test]
    fn particles_fly_outward_and_fade() {
        let early = particles_at(MS(20), 0, 1.0);
        let late = particles_at(MS(300), 0, 1.0);
        assert_eq!(early.len(), PARTICLES);
        let reach = |particle: &Particle| particle.offset.0.hypot(particle.offset.1);
        assert!(reach(&late[0]) > reach(&early[0]) + 20.0);
        assert!(late[0].alpha < early[0].alpha && late[0].radius < early[0].radius);
        assert!(particles_at(MS(420), 0, 1.0).is_empty());
        let twisted = particles_at(MS(20), 1, 1.0);
        assert!((twisted[0].hue - early[0].hue).abs() > 0.01);
    }

    #[test]
    fn the_centre_dot_lasts_one_hundred_and_twenty_milliseconds() {
        assert!(crate::easing::close(dot_alpha(Duration::ZERO), 1.0));
        assert!(dot_alpha(MS(110)) > 0.0);
        assert!(crate::easing::close(dot_alpha(MS(120)), 0.0));
    }

    #[test]
    fn double_clicks_add_a_second_burst_ninety_milliseconds_later() {
        assert_eq!(ripple_life(1), MS(420));
        assert_eq!(ripple_life(2), MS(510));
        let ink = |bitmap: &Bitmap| bitmap.pixels.iter().map(|pixel| u64::from(pixel >> 24)).sum::<u64>();
        assert!(ink(&painted(150, 2).0) > ink(&painted(150, 1).0) + 2000);
    }

    #[test]
    fn ripples_are_premultiplied_coloured_and_clear_at_the_edges() {
        let (bitmap, reach) = painted(60, 2);
        assert_eq!(bitmap.pixel(0, 0), 0);
        assert!(bitmap.pixels.iter().all(|pixel| (0..3).all(|channel| (pixel >> (channel * 8)) & 0xFF <= pixel >> 24)));
        assert!(bitmap.pixel(reach as usize, reach as usize) >> 24 > 240);
        let colours: std::collections::HashSet<u32> =
            bitmap.pixels.iter().filter(|pixel| *pixel >> 24 > 200).map(|pixel| pixel & 0x00F0_F0F0).collect();
        assert!(colours.len() > 3);
    }
}
