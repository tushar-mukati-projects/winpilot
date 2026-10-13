use std::time::Duration;

use winpilot_core::{Point, Rect};

use super::{
    chip_text::ChipText, driving::Driving, focus_outline::FocusLook, glow_tints::GlowLook, presence::Tone,
    trail::TrailDot,
};

pub(crate) const MAX_RIPPLES: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BorderFrame {
    pub(crate) rect: Rect,
    pub(crate) tone: Tone,
    pub(crate) scale: f32,
    pub(crate) alpha: f32,
    pub(crate) glow: GlowLook,
    pub(crate) steady: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PillFrame {
    pub(crate) anchor: Rect,
    pub(crate) label_anchor: Rect,
    pub(crate) tone: Tone,
    pub(crate) driving: Driving,
    pub(crate) scale: f32,
    pub(crate) alpha: f32,
    pub(crate) pulse: f32,
    pub(crate) spin: f32,
    pub(crate) entrance: f32,
    pub(crate) steady: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ActivityFrame<'a> {
    pub(crate) anchor: Rect,
    pub(crate) hint: &'a str,
    pub(crate) scale: f32,
    pub(crate) alpha: f32,
    pub(crate) pulse: f32,
    pub(crate) spin: f32,
    pub(crate) entrance: f32,
    pub(crate) steady: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RippleFrame {
    pub(crate) center: Point,
    pub(crate) elapsed: Duration,
    pub(crate) rings: u8,
    pub(crate) scale: f32,
    pub(crate) animated: bool,
    pub(crate) serial: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TrailFrame<'a> {
    pub(crate) dots: &'a [TrailDot],
    pub(crate) elapsed: Duration,
    pub(crate) serial: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FocusFrame {
    pub(crate) element: Rect,
    pub(crate) scale: f32,
    pub(crate) alpha: f32,
    pub(crate) look: FocusLook,
    pub(crate) steady: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BadgeFrame {
    pub(crate) at: Point,
    pub(crate) alpha: f32,
    pub(crate) scale: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ChipsFrame<'a> {
    pub(crate) rect: Rect,
    pub(crate) text: &'a ChipText,
    pub(crate) alpha: f32,
    pub(crate) scale: f32,
    pub(crate) serial: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ScanFrame {
    pub(crate) rect: Rect,
    pub(crate) scale: f32,
    pub(crate) elapsed: Duration,
    pub(crate) animated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RecordingFrame {
    pub(crate) rect: Rect,
    pub(crate) scale: f32,
    pub(crate) seconds: u64,
    pub(crate) pulse: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ViewfinderFrame {
    pub(crate) rect: Rect,
    pub(crate) scale: f32,
    pub(crate) elapsed: Duration,
    pub(crate) animated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct Frame<'a> {
    pub(crate) border: Option<BorderFrame>,
    pub(crate) pill: Option<PillFrame>,
    pub(crate) activity: Option<ActivityFrame<'a>>,
    pub(crate) ripples: [Option<RippleFrame>; MAX_RIPPLES],
    pub(crate) trail: Option<TrailFrame<'a>>,
    pub(crate) focus: Option<FocusFrame>,
    pub(crate) recording: Option<RecordingFrame>,
    pub(crate) viewfinder: Option<ViewfinderFrame>,
    pub(crate) badge: Option<BadgeFrame>,
    pub(crate) chips: Option<ChipsFrame<'a>>,
    pub(crate) scan: Option<ScanFrame>,
}
