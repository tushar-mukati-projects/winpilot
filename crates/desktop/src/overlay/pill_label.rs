use deckhand_core::Result;

use super::{
    pill_layout::PillMetrics,
    pill_words::{PillWords, Wording, pill_words},
    presence::Tone,
    text_mask::TextMask,
    text_render::{PillFonts, render_text},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PillLabel {
    pub(crate) lead: TextMask,
    pub(crate) hint: Option<TextMask>,
}

impl PillLabel {
    pub(crate) fn width(&self) -> i32 {
        let hint = self.hint.as_ref().map_or(0, |hint| hint.width);
        i32::try_from(self.lead.width.saturating_add(hint)).unwrap_or(i32::MAX)
    }

    pub(crate) fn height(&self) -> i32 {
        let hint = self.hint.as_ref().map_or(0, |hint| hint.height);
        i32::try_from(self.lead.height.max(hint)).unwrap_or(i32::MAX)
    }
}

pub(crate) struct Speaker<'a> {
    pub(crate) label: &'a str,
    pub(crate) hotkey: Option<&'a str>,
}

pub(crate) fn render_label(fonts: &PillFonts, words: &PillWords) -> Result<PillLabel> {
    let lead = render_text(&words.lead, fonts.lead())?;
    let hint = words.hint.as_deref().map(|hint| render_text(hint, fonts.hint())).transpose()?;
    Ok(PillLabel { lead, hint })
}

pub(crate) fn fitting_label(
    fonts: &PillFonts,
    metrics: &PillMetrics,
    tone: Tone,
    room: i32,
    speaker: &Speaker<'_>,
) -> Result<(PillLabel, Wording)> {
    let full = render_label(fonts, &pill_words(tone, Wording::Full, speaker.label, speaker.hotkey))?;
    if metrics.fits(room, full.width()) {
        return Ok((full, Wording::Full));
    }
    let short = pill_words(tone, Wording::Short, speaker.label, speaker.hotkey);
    Ok((render_label(fonts, &short)?, Wording::Short))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEAKER: Speaker<'static> = Speaker { label: "Claude", hotkey: Some("Ctrl+Alt+Shift+Backspace") };

    fn label_for(width: i32) -> (PillLabel, Wording) {
        let metrics = PillMetrics::at_scale(1.0);
        let fonts = PillFonts::new(metrics.font_px).unwrap();
        fitting_label(&fonts, &metrics, Tone::Working, width, &SPEAKER).unwrap()
    }

    #[test]
    fn wide_windows_get_the_full_label() {
        let (label, wording) = label_for(1200);
        assert_eq!(wording, Wording::Full);
        assert!(label.width() > 300);
    }

    #[test]
    fn narrow_windows_get_the_short_label() {
        let (label, wording) = label_for(300);
        assert_eq!(wording, Wording::Short);
        assert!(PillMetrics::at_scale(1.0).fits(300, label.width()));
    }
}
