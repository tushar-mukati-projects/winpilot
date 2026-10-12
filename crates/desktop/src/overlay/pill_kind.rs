use deckhand_core::Result;

use super::{
    color::{Rgb, WHITE},
    pill_label::{PillLabel, Speaker, fitting_label, render_label},
    pill_layout::{Align, PillMetrics},
    pill_paint::{GLASS_PILL, PillStyle},
    pill_words::recording_words,
    presence::Tone,
    text_render::PillFonts,
};

const CRIMSON_PILL: PillStyle =
    PillStyle { fill: Rgb::new(120, 14, 38), fill_alpha: 0.9, lead: WHITE, hint: Rgb::new(255, 222, 228) };

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Glyph {
    Sparkle,
    Dot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PillKind {
    Presence { tone: Tone, room: i32 },
    Recording { seconds: u64 },
}

impl PillKind {
    pub(crate) const fn style(self) -> PillStyle {
        match self {
            Self::Presence { .. } => GLASS_PILL,
            Self::Recording { .. } => CRIMSON_PILL,
        }
    }

    pub(crate) const fn align(self) -> Align {
        match self {
            Self::Presence { .. } => Align::TopCenter,
            Self::Recording { .. } => Align::TopRight,
        }
    }

    pub(crate) const fn glyph(self) -> Glyph {
        match self {
            Self::Presence { .. } => Glyph::Sparkle,
            Self::Recording { .. } => Glyph::Dot,
        }
    }

    pub(crate) const fn alarming(self) -> bool {
        matches!(self, Self::Presence { tone: Tone::Stopped, .. } | Self::Recording { .. })
    }

    pub(crate) fn label(self, fonts: &PillFonts, metrics: &PillMetrics, speaker: &Speaker<'_>) -> Result<PillLabel> {
        match self {
            Self::Presence { tone, room } => fitting_label(fonts, metrics, tone, room, speaker),
            Self::Recording { seconds } => render_label(fonts, &recording_words(speaker.label, seconds)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presence_pills_are_glass_and_centred_and_recording_pills_crimson_and_right() {
        let presence = PillKind::Presence { tone: Tone::Working, room: 800 };
        let recording = PillKind::Recording { seconds: 3 };
        assert_eq!(
            (presence.style(), presence.align(), presence.glyph()),
            (GLASS_PILL, Align::TopCenter, Glyph::Sparkle)
        );
        assert_eq!((recording.align(), recording.glyph()), (Align::TopRight, Glyph::Dot));
        assert!(recording.alarming() && !presence.alarming());
        assert!(PillKind::Presence { tone: Tone::Stopped, room: 1 }.alarming());
    }

    #[test]
    fn recording_labels_show_the_elapsed_time() {
        let metrics = PillMetrics::at_scale(1.0);
        let fonts = PillFonts::new(metrics.font_px).unwrap();
        let speaker = Speaker { label: "Claude", hotkey: None };
        let label = PillKind::Recording { seconds: 12 }.label(&fonts, &metrics, &speaker).unwrap();
        assert!(label.hint.is_some_and(|hint| hint.width > 60));
    }
}
