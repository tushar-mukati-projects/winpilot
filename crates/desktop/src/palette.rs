use crate::overlay_color::Rgb;

const STEPS: usize = 256;
const CRIMSON: [[u8; 3]; 4] = [[0xE5, 0x48, 0x4D], [0xB9, 0x1C, 0x1C], [0xF4, 0x3F, 0x5E], [0x9F, 0x12, 0x39]];
const AURORA: [[u8; 3]; 5] =
    [[0x7C, 0x3A, 0xED], [0x4F, 0x46, 0xE5], [0x0E, 0xA5, 0xE9], [0x22, 0xD3, 0xEE], [0xF4, 0x72, 0xB6]];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Palette {
    lut: Vec<Rgb>,
}

impl Palette {
    pub(crate) fn looping(stops: &[[u8; 3]]) -> Self {
        let stops: Vec<Rgb> = stops.iter().map(|[red, green, blue]| Rgb::new(*red, *green, *blue)).collect();
        let stops =
            if stops.is_empty() { AURORA.iter().map(|[r, g, b]| Rgb::new(*r, *g, *b)).collect() } else { stops };
        let lut = (0..STEPS).map(|step| blend_at(&stops, step as f32 / STEPS as f32)).collect();
        Self { lut }
    }

    pub(crate) fn crimson() -> Self {
        Self::looping(&CRIMSON)
    }

    #[cfg(test)]
    pub(crate) fn aurora() -> Self {
        Self::looping(&AURORA)
    }

    pub(crate) fn at(&self, position: f32) -> Rgb {
        let wrapped = if position.is_finite() { position.rem_euclid(1.0) } else { 0.0 };
        let index = ((wrapped * STEPS as f32) as usize).min(STEPS - 1);
        self.lut.get(index).copied().unwrap_or(Rgb::new(255, 255, 255))
    }
}

fn blend_at(stops: &[Rgb], position: f32) -> Rgb {
    let count = stops.len();
    let scaled = position * count as f32;
    let index = (scaled as usize).min(count - 1);
    let amount = scaled - index as f32;
    let (from, to) = (stops[index], stops[(index + 1) % count]);
    let eased = amount * amount * (3.0 - 2.0 * amount);
    from.mix(to, eased)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_palette_passes_through_every_stop_and_loops() {
        let palette = Palette::aurora();
        assert_eq!(palette.at(0.0), Rgb::new(0x7C, 0x3A, 0xED));
        assert_eq!(palette.at(0.2), Rgb::new(0x4F, 0x46, 0xE5));
        assert_eq!(palette.at(1.0), palette.at(0.0));
        assert_eq!(palette.at(-0.8), palette.at(0.2));
    }

    #[test]
    fn neighbouring_samples_change_smoothly() {
        let palette = Palette::looping(&[[255, 0, 0], [0, 0, 255]]);
        let jumps = (0..256).map(|step| {
            let (a, b) = (palette.at(step as f32 / 256.0), palette.at((step + 1) as f32 / 256.0));
            i32::from(a.red).abs_diff(i32::from(b.red)).max(i32::from(a.blue).abs_diff(i32::from(b.blue)))
        });
        assert!(jumps.max().unwrap() <= 6);
    }

    #[test]
    fn single_colours_and_bad_input_still_work() {
        assert_eq!(Palette::looping(&[[9, 9, 9]]).at(0.7), Rgb::new(9, 9, 9));
        assert_eq!(Palette::looping(&[]).at(0.0), Rgb::new(0x7C, 0x3A, 0xED));
        assert_eq!(Palette::crimson().at(f32::NAN), Rgb::new(0xE5, 0x48, 0x4D));
    }
}
