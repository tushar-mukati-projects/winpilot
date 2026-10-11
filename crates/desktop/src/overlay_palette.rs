use crate::overlay_color::Rgb;

pub(crate) const WHITE: Rgb = Rgb::new(255, 255, 255);
pub(crate) const BLACK: Rgb = Rgb::new(0, 0, 0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tone {
    Working,
    Stopped,
}
