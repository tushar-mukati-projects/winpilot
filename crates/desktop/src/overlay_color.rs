#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rgb {
    pub(crate) red: u8,
    pub(crate) green: u8,
    pub(crate) blue: u8,
}

impl Rgb {
    pub(crate) const fn new(red: u8, green: u8, blue: u8) -> Self {
        Self { red, green, blue }
    }

    pub(crate) fn mix(self, other: Self, amount: f32) -> Self {
        let amount = amount.clamp(0.0, 1.0);
        let channel = |from: u8, to: u8| (f32::from(from) + (f32::from(to) - f32::from(from)) * amount).round() as u8;
        Self::new(channel(self.red, other.red), channel(self.green, other.green), channel(self.blue, other.blue))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Premul {
    pub(crate) red: f32,
    pub(crate) green: f32,
    pub(crate) blue: f32,
    pub(crate) alpha: f32,
}

impl Premul {
    pub(crate) fn of(color: Rgb, alpha: f32) -> Self {
        let alpha = if alpha.is_finite() { alpha.clamp(0.0, 1.0) } else { 0.0 };
        let channel = |value: u8| f32::from(value) / 255.0 * alpha;
        Self { red: channel(color.red), green: channel(color.green), blue: channel(color.blue), alpha }
    }

    pub(crate) fn over(self, below: Self) -> Self {
        let keep = 1.0 - self.alpha;
        Self {
            red: self.red + below.red * keep,
            green: self.green + below.green * keep,
            blue: self.blue + below.blue * keep,
            alpha: self.alpha + below.alpha * keep,
        }
    }

    pub(crate) fn scaled(self, factor: f32) -> Self {
        let factor = if factor.is_finite() { factor.clamp(0.0, 1.0) } else { 0.0 };
        Self {
            red: self.red * factor,
            green: self.green * factor,
            blue: self.blue * factor,
            alpha: self.alpha * factor,
        }
    }

    pub(crate) fn to_bgra(self) -> u32 {
        let alpha = to_byte(self.alpha);
        let channel = |value: f32| u32::from(to_byte(value).min(alpha));
        (u32::from(alpha) << 24) | (channel(self.red) << 16) | (channel(self.green) << 8) | channel(self.blue)
    }

    pub(crate) fn from_bgra(pixel: u32) -> Self {
        let channel = |shift: u32| ((pixel >> shift) & 0xFF) as f32 / 255.0;
        Self { red: channel(16), green: channel(8), blue: channel(0), alpha: channel(24) }
    }
}

fn to_byte(value: f32) -> u8 {
    if value.is_finite() { (value.clamp(0.0, 1.0) * 255.0).round() as u8 } else { 0 }
}

pub(crate) fn alpha_byte(alpha: f32) -> u8 {
    to_byte(alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORANGE: Rgb = Rgb::new(0xD9, 0x77, 0x57);

    #[test]
    fn opaque_colors_keep_their_channels() {
        assert_eq!(Premul::of(ORANGE, 1.0).to_bgra(), 0xFFD9_7757);
        assert_eq!(Premul::of(ORANGE, 0.0).to_bgra(), 0);
    }

    #[test]
    fn channels_never_exceed_alpha() {
        for step in 0..=20 {
            let pixel = Premul::of(Rgb::new(255, 255, 255), step as f32 / 20.0).to_bgra();
            let alpha = pixel >> 24;
            assert!((0..3).all(|index| (pixel >> (index * 8)) & 0xFF <= alpha));
        }
    }

    #[test]
    fn over_composites_premultiplied_layers() {
        let top = Premul::of(Rgb::new(255, 0, 0), 0.5);
        let below = Premul::of(Rgb::new(0, 0, 255), 1.0);
        let mixed = top.over(below);
        assert!((mixed.alpha - 1.0).abs() < 1e-6);
        assert!((mixed.red - 0.5).abs() < 1e-6 && (mixed.blue - 0.5).abs() < 1e-6);
    }

    #[test]
    fn bad_numbers_become_transparent() {
        assert_eq!(Premul::of(ORANGE, f32::NAN).to_bgra(), 0);
        assert_eq!(Premul::of(ORANGE, 1.0).scaled(f32::INFINITY).to_bgra(), 0);
    }

    #[test]
    fn mixing_moves_toward_the_other_color() {
        assert_eq!(ORANGE.mix(Rgb::new(255, 255, 255), 0.0), ORANGE);
        assert_eq!(ORANGE.mix(Rgb::new(255, 255, 255), 1.0), Rgb::new(255, 255, 255));
        assert_eq!(Premul::from_bgra(0x8040_2010).to_bgra(), 0x8040_2010);
    }
}
