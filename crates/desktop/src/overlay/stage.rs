use deckhand_core::{Rect, Result};
use windows::Win32::{Foundation::HINSTANCE, UI::WindowsAndMessaging::MSG};

use super::{
    badge_layer::BadgeLayer,
    border_geometry::BorderMetrics,
    border_layers::{BorderLayers, BorderLook},
    chips_layer::ChipsLayer,
    color::alpha_byte,
    focus_layers::FocusLayers,
    layer::Layer,
    monitor_at::work_area_around,
    palette::Palette,
    pill_kind::PillKind,
    pill_layer::{PillLayer, PillShow, PillVoice},
    presence::Tone,
    ripple_layers::RippleLayers,
    scan_layers::ScanLayers,
    scene_frame::Frame,
    strip_layers::StripLayers,
    tint_window::TintWindow,
    trail_layers::TrailLayers,
    viewfinder_layers::ViewfinderLayers,
    window::Capture,
};

const STEADY_EVERY: u64 = 3;

struct Factory {
    instance: HINSTANCE,
}

impl Factory {
    fn layer(&self, capture: Capture) -> Result<Layer> {
        Layer::create(self.instance, capture)
    }

    fn layers<const N: usize>(&self, capture: Capture) -> Result<[Layer; N]> {
        let made: Vec<Layer> = (0..N).map(|_| self.layer(capture)).collect::<Result<_>>()?;
        made.try_into().map_err(|_| deckhand_core::Error::internal("creating overlay windows"))
    }

    fn strips(&self, capture: Capture) -> Result<StripLayers> {
        StripLayers::create(self.instance, capture)
    }

    fn tint(&self) -> Result<TintWindow> {
        TintWindow::create(self.instance)
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
    pub(crate) fn create(instance: HINSTANCE, presence: Capture, cast: Cast) -> Result<Self> {
        let make = Factory { instance };
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
        Ok(stage)
    }

    pub(crate) fn every_layer_excluded_from_capture(&self) -> bool {
        self.layers().all(Layer::is_excluded_from_capture) && self.viewfinder.tint().is_excluded_from_capture()
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
        self.show_private(frame, screen, paint_tick, concealed);
        self.reveal(concealed);
    }

    pub(crate) fn conceal(&mut self) -> bool {
        let hid_layer = self.reveal(true);
        let hid_tint = self.viewfinder.conceal_tint();
        hid_layer || hid_tint
    }

    fn reveal(&mut self, concealed: bool) -> bool {
        let mut hid = false;
        for layer in self.layers_mut() {
            hid |= layer.reveal(concealed);
        }
        hid
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
        self.chips.release_memory();
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
                work_area_around(pill.anchor).unwrap_or(screen),
            ),
            None => self.pill.hide(),
        }
    }

    fn show_private(&mut self, frame: &Frame<'_>, screen: Rect, paint_tick: bool, concealed: bool) {
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
                work_area_around(recording.rect).unwrap_or(screen),
            ),
            None => self.recording.hide(),
        }
        match frame.viewfinder {
            Some(viewfinder) => self.viewfinder.show(&viewfinder, &self.palette, screen, concealed),
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
