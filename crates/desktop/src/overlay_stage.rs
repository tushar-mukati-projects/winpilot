use deckhand_core::{Rect, Result};
use windows::Win32::{Foundation::HINSTANCE, UI::WindowsAndMessaging::MSG};

use crate::{
    badge_layer::BadgeLayer,
    border_geometry::BorderMetrics,
    border_layers::{BorderLayers, BorderLook},
    chips_layer::ChipsLayer,
    focus_layers::FocusLayers,
    overlay_color::alpha_byte,
    overlay_frame::Frame,
    overlay_layer::Layer,
    overlay_palette::Tone,
    overlay_window::Capture,
    palette::Palette,
    pill_kind::PillKind,
    pill_layer::{PillLayer, PillShow, PillVoice},
    ripple_layers::RippleLayers,
    scan_layers::ScanLayers,
    strip_layers::StripLayers,
    tint_window::TintWindow,
    trail_layers::TrailLayers,
    viewfinder_layers::ViewfinderLayers,
};

const STEADY_EVERY: u64 = 3;

struct Factory {
    instance: HINSTANCE,
    applied: bool,
}

impl Factory {
    fn layer(&mut self, capture: Capture) -> Result<Layer> {
        let (layer, applied) = Layer::create(self.instance, capture)?;
        self.applied &= applied;
        Ok(layer)
    }

    fn layers<const N: usize>(&mut self, capture: Capture) -> Result<[Layer; N]> {
        let made: Vec<Layer> = (0..N).map(|_| self.layer(capture)).collect::<Result<_>>()?;
        made.try_into().map_err(|_| deckhand_core::Error::internal("creating overlay windows"))
    }

    fn strips(&mut self, capture: Capture) -> Result<StripLayers> {
        let (strips, applied) = StripLayers::create(self.instance, capture)?;
        self.applied &= applied;
        Ok(strips)
    }

    fn tint(&mut self) -> Result<TintWindow> {
        let (tint, applied) = TintWindow::create(self.instance)?;
        self.applied &= applied;
        Ok(tint)
    }
}

pub(crate) struct Cast {
    pub(crate) label: String,
    pub(crate) hotkey: Option<String>,
    pub(crate) palette: Palette,
}

pub(crate) struct Stage {
    palette: Palette,
    alarm: Palette,
    border: BorderLayers,
    focus: FocusLayers,
    pill: PillLayer,
    ripples: RippleLayers,
    trail: TrailLayers,
    badge: BadgeLayer,
    chips: ChipsLayer,
    scan: ScanLayers,
    recording: PillLayer,
    viewfinder: ViewfinderLayers,
}

impl Stage {
    pub(crate) fn create(instance: HINSTANCE, presence: Capture, cast: Cast) -> Result<(Self, bool)> {
        let mut make = Factory { instance, applied: true };
        let voice = |cast: &Cast| PillVoice {
            label: cast.label.clone(),
            hotkey: cast.hotkey.clone(),
            palette: cast.palette.clone(),
            alarm: Palette::crimson(),
        };
        let stage = Self {
            border: BorderLayers::new(make.layers(presence)?),
            focus: FocusLayers::new(make.strips(presence)?, make.layer(presence)?),
            pill: PillLayer::new(make.layer(presence)?, voice(&cast)),
            ripples: RippleLayers::new(make.layers(presence)?),
            trail: TrailLayers::new(make.layers(presence)?),
            badge: BadgeLayer::new(make.layer(presence)?, cast.label.clone()),
            chips: ChipsLayer::new(make.layer(presence)?),
            scan: ScanLayers::new(make.layer(presence)?, make.strips(presence)?),
            recording: PillLayer::new(make.layer(Capture::Excluded)?, voice(&cast)),
            viewfinder: ViewfinderLayers::new(
                make.layers(Capture::Excluded)?,
                make.strips(Capture::Excluded)?,
                make.tint()?,
            ),
            alarm: Palette::crimson(),
            palette: cast.palette,
        };
        Ok((stage, make.applied))
    }

    pub(crate) fn owns_timer(&self, message: &MSG) -> bool {
        self.pill.layer().owns(message)
    }

    pub(crate) fn timer_layer(&self) -> &Layer {
        self.pill.layer()
    }

    pub(crate) fn show(&mut self, frame: &Frame<'_>, screen: Rect, concealed: bool, tick: u64) {
        let paint_tick = tick.is_multiple_of(STEADY_EVERY);
        self.show_presence(frame, screen, tick);
        self.ripples.show(&frame.ripples, &self.palette);
        self.trail.show(frame.trail.as_ref(), &self.palette);
        match frame.focus {
            Some(focus) => self.focus.show(&focus, &self.palette, paint_tick, screen),
            None => self.focus.hide(),
        }
        match frame.badge {
            Some(badge) => self.badge.show(&badge, &self.palette),
            None => self.badge.hide(),
        }
        match frame.chips {
            Some(chips) => self.chips.show(&chips, &self.palette),
            None => self.chips.hide(),
        }
        match frame.scan {
            Some(scan) => self.scan.show(&scan, &self.palette, screen),
            None => self.scan.hide(),
        }
        self.show_private(frame, screen, paint_tick);
        self.reveal(concealed);
    }

    pub(crate) fn reveal(&mut self, concealed: bool) {
        for layer in self.layers_mut() {
            layer.reveal(concealed);
        }
    }

    pub(crate) fn raise(&self) {
        for layer in self.layers() {
            layer.raise();
        }
    }

    pub(crate) fn release_memory(&mut self) {
        self.pill.release_memory();
        self.recording.release_memory();
        self.badge.release_memory();
        self.viewfinder.release_memory();
        for layer in self.layers_mut() {
            layer.release_memory();
        }
    }

    fn show_presence(&mut self, frame: &Frame<'_>, screen: Rect, tick: u64) {
        let paint_tick = tick.is_multiple_of(STEADY_EVERY);
        match frame.border {
            Some(border) => {
                let palette = if border.tone == Tone::Stopped { &self.alarm } else { &self.palette };
                let look = BorderLook {
                    rect: border.rect,
                    metrics: BorderMetrics::at_scale(border.scale),
                    palette,
                    glow: border.glow,
                    alpha: alpha_byte(border.alpha),
                    tick,
                    steady: border.steady,
                };
                self.border.show(&look, screen);
            }
            None => self.border.hide(),
        }
        match frame.pill {
            Some(pill) => self.pill.show(
                &PillShow {
                    kind: PillKind::Presence { tone: pill.tone, room: pill.label_anchor.width },
                    anchor: pill.anchor,
                    scale: pill.scale,
                    alpha: pill.alpha,
                    pulse: pill.pulse,
                    spin: pill.spin,
                    entrance: pill.entrance,
                    repaint: paint_tick || !pill.steady,
                },
                screen,
            ),
            None => self.pill.hide(),
        }
    }

    fn show_private(&mut self, frame: &Frame<'_>, screen: Rect, paint_tick: bool) {
        match frame.recording {
            Some(recording) => self.recording.show(
                &PillShow {
                    kind: PillKind::Recording { seconds: recording.seconds },
                    anchor: recording.rect,
                    scale: recording.scale,
                    alpha: 1.0,
                    pulse: recording.pulse,
                    spin: recording.pulse * 0.25,
                    entrance: 1.0,
                    repaint: paint_tick,
                },
                screen,
            ),
            None => self.recording.hide(),
        }
        match frame.viewfinder {
            Some(viewfinder) => self.viewfinder.show(&viewfinder, &self.palette, screen),
            None => self.viewfinder.hide(),
        }
    }

    fn layers(&self) -> impl Iterator<Item = &Layer> {
        self.border
            .layers()
            .iter()
            .chain(self.focus.layers())
            .chain([self.pill.layer(), self.recording.layer(), self.badge.layer(), self.chips.layer()])
            .chain(self.ripples.layers())
            .chain(self.trail.layers())
            .chain(self.scan.layers())
            .chain(self.viewfinder.layers())
    }

    fn layers_mut(&mut self) -> impl Iterator<Item = &mut Layer> {
        self.border
            .layers_mut()
            .iter_mut()
            .chain(self.focus.layers_mut())
            .chain([self.pill.layer_mut(), self.recording.layer_mut(), self.badge.layer_mut(), self.chips.layer_mut()])
            .chain(self.ripples.layers_mut().iter_mut())
            .chain(self.trail.layers_mut().iter_mut())
            .chain(self.scan.layers_mut())
            .chain(self.viewfinder.layers_mut())
    }
}
