use winpilot_core::Result;

use super::{
    activity_words::activity_words,
    color::{Rgb, WHITE},
    pill_label::{PillLabel, PresenceLook, Speaker, fitting_label, render_label},
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
    Presence(PresenceLook),
    Recording { seconds: u64 },
    Activity,
}

impl PillKind {
    pub(crate) const fn style(self) -> PillStyle {
        match self {
            Self::Presence(_) | Self::Activity => GLASS_PILL,
            Self::Recording { .. } => CRIMSON_PILL,
        }
    }

    pub(crate) const fn align(self) -> Align {
        match self {
            Self::Presence(_) => Align::TopCenter,
            Self::Recording { .. } => Align::TopRight,
            Self::Activity => Align::BottomCenter,
        }
    }

    pub(crate) const fn glyph(self) -> Glyph {
        match self {
            Self::Presence(_) | Self::Activity => Glyph::Sparkle,
            Self::Recording { .. } => Glyph::Dot,
        }
    }

    pub(crate) const fn alarming(self) -> bool {
        matches!(self, Self::Presence(PresenceLook { tone: Tone::Stopped, .. }) | Self::Recording { .. })
    }

    pub(crate) fn label(self, fonts: &PillFonts, metrics: &PillMetrics, speaker: &Speaker<'_>) -> Result<PillLabel> {
        match self {
            Self::Presence(look) => fitting_label(fonts, metrics, look, speaker),
            Self::Recording { seconds } => render_label(fonts, &recording_words(speaker.label, seconds)),
            Self::Activity => render_label(fonts, &activity_words(speaker.label, speaker.detail)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::overlay::presence::Driving;

    const WORKING: PresenceLook = PresenceLook { tone: Tone::Working, driving: Driving::Foreground, room: 800 };

    #[test]
    fn presence_pills_are_glass_and_centred_and_recording_pills_crimson_and_right() {
        let presence = PillKind::Presence(WORKING);
        let recording = PillKind::Recording { seconds: 3 };
        assert_eq!(
            (presence.style(), presence.align(), presence.glyph()),
            (GLASS_PILL, Align::TopCenter, Glyph::Sparkle)
        );
        assert_eq!((recording.align(), recording.glyph()), (Align::TopRight, Glyph::Dot));
        assert!(recording.alarming() && !presence.alarming());
        assert!(PillKind::Presence(PresenceLook { tone: Tone::Stopped, room: 1, ..WORKING }).alarming());
    }

    #[test]
    fn activity_pills_are_glass_calm_and_sit_at_the_bottom() {
        let activity = PillKind::Activity;
        assert_eq!(
            (activity.style(), activity.align(), activity.glyph()),
            (GLASS_PILL, Align::BottomCenter, Glyph::Sparkle)
        );
        assert!(!activity.alarming());
    }

    #[test]
    fn recording_labels_show_the_elapsed_time() {
        let metrics = PillMetrics::at_scale(1.0);
        let fonts = PillFonts::new(metrics.font_px).unwrap();
        let speaker = Speaker { label: "Claude", hotkey: None, detail: "" };
        let label = PillKind::Recording { seconds: 12 }.label(&fonts, &metrics, &speaker).unwrap();
        assert!(label.hint.is_some_and(|hint| hint.width > 60));
    }

    #[test]
    fn activity_labels_carry_the_detail() {
        let metrics = PillMetrics::at_scale(1.0);
        let fonts = PillFonts::new(metrics.font_px).unwrap();
        let speaker = Speaker { label: "Claude", hotkey: None, detail: " \u{00B7} working in CapCut" };
        let label = PillKind::Activity.label(&fonts, &metrics, &speaker).unwrap();
        assert!(label.hint.is_some_and(|hint| hint.width > 80));
    }
}
