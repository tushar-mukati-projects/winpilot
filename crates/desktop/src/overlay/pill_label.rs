use winpilot_core::Result;

use super::{
    driving::Driving,
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
    pub(crate) detail: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PresenceLook {
    pub(crate) tone: Tone,
    pub(crate) driving: Driving,
    pub(crate) room: i32,
}

pub(crate) fn render_label(fonts: &PillFonts, words: &PillWords) -> Result<PillLabel> {
    let lead = render_text(&words.lead, fonts.lead())?;
    let hint = words.hint.as_deref().map(|hint| render_text(hint, fonts.hint())).transpose()?;
    Ok(PillLabel { lead, hint })
}

pub(crate) fn fitting_label(
    fonts: &PillFonts,
    metrics: &PillMetrics,
    look: PresenceLook,
    speaker: &Speaker<'_>,
) -> Result<PillLabel> {
    let worded = |wording| pill_words(look.tone, look.driving, wording, speaker.label, speaker.hotkey);
    let full = render_label(fonts, &worded(Wording::Full))?;
    if metrics.fits(look.room, full.width()) {
        return Ok(full);
    }
    render_label(fonts, &worded(Wording::Short))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEAKER: Speaker<'static> = Speaker { label: "Claude", hotkey: Some("Ctrl+Alt+Esc"), detail: "" };

    fn label_for(width: i32) -> (PillLabel, PillLabel) {
        let metrics = PillMetrics::at_scale(1.0);
        let fonts = PillFonts::new(metrics.font_px).unwrap();
        let look = PresenceLook { tone: Tone::Working, driving: Driving::Foreground, room: width };
        let fitted = fitting_label(&fonts, &metrics, look, &SPEAKER).unwrap();
        (fitted, worded(&fonts, Wording::Full))
    }

    fn worded(fonts: &PillFonts, wording: Wording) -> PillLabel {
        render_label(fonts, &pill_words(Tone::Working, Driving::Foreground, wording, SPEAKER.label, SPEAKER.hotkey))
            .unwrap()
    }

    #[test]
    fn wide_windows_get_the_full_label() {
        let (full, expected) = label_for(1200);
        assert_eq!(full, expected);
        let (short, _) = label_for(300);
        assert!(full.width() > short.width() + 100, "{} vs {}", full.width(), short.width());
    }

    #[test]
    fn narrow_windows_get_the_short_label() {
        let (label, full) = label_for(300);
        assert_ne!(label, full);
        let fonts = PillFonts::new(PillMetrics::at_scale(1.0).font_px).unwrap();
        assert_eq!(label, worded(&fonts, Wording::Short));
        assert!(PillMetrics::at_scale(1.0).fits(300, label.width()));
    }
}
