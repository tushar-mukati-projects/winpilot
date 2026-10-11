use deckhand_core::{Error, Result};
use windows::{
    Win32::{
        Foundation::{COLORREF, SIZE},
        Graphics::Gdi::{
            ANTIALIASED_QUALITY, CLIP_DEFAULT_PRECIS, CreateFontW, DEFAULT_CHARSET, DEFAULT_PITCH, FF_SWISS, GdiFlush,
            GetTextExtentPoint32W, GetTextFaceW, GetTextMetricsW, OUT_TT_PRECIS, SetBkMode, SetTextColor, TEXTMETRICW,
            TRANSPARENT, TextOutW,
        },
    },
    core::{PCWSTR, w},
};

use super::{
    gdi::{DibSection, Font, MemoryDc, Selection},
    text_mask::TextMask,
};

const FACES: [PCWSTR; 2] = [w!("Segoe UI Variable Text"), w!("Segoe UI")];
const SEMIBOLD: i32 = 600;
const REGULAR: i32 = 400;
const WHITE_TEXT: COLORREF = COLORREF(0x00FF_FFFF);
const INVALID_COLOR: u32 = 0xFFFF_FFFF;
const SIDE_ROOM: i32 = 1;

pub(crate) struct PillFonts {
    pub(crate) px: i32,
    lead: Font,
    hint: Font,
}

impl PillFonts {
    pub(crate) fn new(px: i32) -> Result<Self> {
        Ok(Self { px, lead: best_font(px, SEMIBOLD)?, hint: best_font(px, REGULAR)? })
    }

    pub(crate) fn lead(&self) -> &Font {
        &self.lead
    }

    pub(crate) fn hint(&self) -> &Font {
        &self.hint
    }
}

fn best_font(px: i32, weight: i32) -> Result<Font> {
    for face in FACES {
        let font = create_font(face, px, weight)?;
        if uses_face(&font, face)? {
            return Ok(font);
        }
    }
    create_font(PCWSTR::null(), px, weight)
}

fn create_font(face: PCWSTR, px: i32, weight: i32) -> Result<Font> {
    let pitch_and_family = u32::from(DEFAULT_PITCH.0) | u32::from(FF_SWISS.0);
    Font::adopt(unsafe {
        CreateFontW(
            -px.max(1),
            0,
            0,
            0,
            weight,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_TT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            ANTIALIASED_QUALITY,
            pitch_and_family,
            face,
        )
    })
}

fn uses_face(font: &Font, face: PCWSTR) -> Result<bool> {
    let dc = MemoryDc::new()?;
    let _font = Selection::new(&dc, font.object())?;
    let mut name = [0u16; 64];
    let copied = unsafe { GetTextFaceW(dc.handle(), Some(&mut name)) };
    let length = usize::try_from(copied).unwrap_or(0).min(name.len());
    let actual = String::from_utf16_lossy(&name[..length]);
    let wanted = unsafe { face.to_string() }.unwrap_or_default();
    Ok(actual.trim_end_matches('\0').eq_ignore_ascii_case(&wanted))
}

pub(crate) fn render_text(text: &str, font: &Font) -> Result<TextMask> {
    let wide: Vec<u16> = text.encode_utf16().collect();
    if wide.is_empty() {
        return Ok(TextMask::default());
    }
    let dc = MemoryDc::new()?;
    let _font = Selection::new(&dc, font.object())?;
    let (width, height) = measure(&dc, &wide)?;
    let mut section = DibSection::new(&dc, width, height)?;
    section.pixels().fill(0);
    let _bitmap = Selection::new(&dc, section.object())?;
    draw_white(&dc, &wide)?;
    let _flushed = unsafe { GdiFlush() };
    let (columns, rows) = (section.width(), section.height());
    Ok(TextMask::from_gray_bgra(columns, rows, section.pixels()))
}

fn measure(dc: &MemoryDc, wide: &[u16]) -> Result<(i32, i32)> {
    let mut extent = SIZE::default();
    if !unsafe { GetTextExtentPoint32W(dc.handle(), wide, &raw mut extent) }.as_bool() {
        return Err(Error::internal("measuring the overlay label"));
    }
    let mut metrics = TEXTMETRICW::default();
    if !unsafe { GetTextMetricsW(dc.handle(), &raw mut metrics) }.as_bool() {
        return Err(Error::internal("reading the overlay font metrics"));
    }
    Ok((extent.cx.saturating_add(SIDE_ROOM * 2).max(1), metrics.tmHeight.max(extent.cy).max(1)))
}

fn draw_white(dc: &MemoryDc, wide: &[u16]) -> Result<()> {
    let mode_set = unsafe { SetBkMode(dc.handle(), TRANSPARENT) } != 0;
    let color_set = unsafe { SetTextColor(dc.handle(), WHITE_TEXT) }.0 != INVALID_COLOR;
    let drawn = mode_set && color_set && unsafe { TextOutW(dc.handle(), SIDE_ROOM, 0, wide) }.as_bool();
    if drawn { Ok(()) } else { Err(Error::internal("drawing the overlay label")) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_render_as_coverage_masks() {
        let fonts = PillFonts::new(17).unwrap();
        let mask = render_text("Claude is controlling", fonts.lead()).unwrap();
        assert!(mask.width > 100 && mask.height >= 17);
        assert!(mask.coverage.contains(&255));
        assert!(mask.coverage.iter().any(|cover| *cover > 0 && *cover < 255));
        assert_eq!(mask.at(0, 0), 0);
    }

    #[test]
    fn semibold_lead_text_is_heavier_than_the_hint() {
        let fonts = PillFonts::new(17).unwrap();
        let ink =
            |font: &Font| render_text("controlling", font).unwrap().coverage.iter().map(|c| u32::from(*c)).sum::<u32>();
        assert!(ink(fonts.lead()) > ink(fonts.hint()));
    }

    #[test]
    fn empty_labels_are_empty_masks() {
        let fonts = PillFonts::new(17).unwrap();
        assert_eq!(render_text("", fonts.hint()).unwrap(), TextMask::default());
    }
}
