mod badge_layer;
mod border_layers;
mod chips_layer;
mod focus_layers;
mod pill_layer;
mod pills;
mod ripple_layers;
mod scan_layers;
mod strip_layers;
mod trail_layers;
mod viewfinder_layers;

use windows::Win32::{Foundation::HINSTANCE, UI::WindowsAndMessaging::MSG};
use winpilot_core::{Rect, Result};

use badge_layer::BadgeLayer;
use border_layers::{BorderLayers, BorderLook};
use chips_layer::ChipsLayer;
use focus_layers::FocusLayers;
use pill_layer::{PillLayer, PillVoice};
use ripple_layers::RippleLayers;
use scan_layers::ScanLayers;
use strip_layers::StripLayers;
use trail_layers::TrailLayers;
use viewfinder_layers::ViewfinderLayers;

use super::{
    border_geometry::BorderMetrics, color::alpha_byte, layer::Layer, palette::Palette, presence::Tone,
    scene_frame::Frame, tint_window::TintWindow, window::Capture,
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
        made.try_into().map_err(|_| winpilot_core::Error::internal("creating overlay windows"))
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
    activity: PillLayer,
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
        };
        let stage = Self {
            border: BorderLayers::new(make.layers(presence)?),
            focus: FocusLayers::new(make.strips(presence)?, make.layer(presence)?),
            pill: PillLayer::new(make.layer(presence)?, voice(&cast)),
            activity: PillLayer::new(make.layer(presence)?, voice(&cast)),
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

    pub(crate) fn relabel(&mut self, label: &str) {
        self.pill.relabel(label);
        self.activity.relabel(label);
        self.recording.relabel(label);
        self.badge.relabel(label);
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
        self.show_border(frame, screen, tick);
        self.show_presence(frame, screen, paint_tick);
        self.show_activity(frame, paint_tick);
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
        self.show_recording(frame, screen, paint_tick);
        match frame.viewfinder {
            Some(viewfinder) => self.viewfinder.show(&viewfinder, &self.palette, screen, concealed),
            None => self.viewfinder.hide(),
        }
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
        self.activity.release_memory();
        self.recording.release_memory();
        self.badge.release_memory();
        self.chips.release_memory();
        self.viewfinder.release_memory();
        for layer in self.layers_mut() {
            layer.release_memory();
        }
    }

    fn show_border(&mut self, frame: &Frame<'_>, screen: Rect, tick: u64) {
        let Some(border) = frame.border else {
            self.border.hide();
            return;
        };
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

    fn layers(&self) -> impl Iterator<Item = &Layer> {
        self.border
            .layers()
            .iter()
            .chain(self.focus.layers())
            .chain([
                self.pill.layer(),
                self.activity.layer(),
                self.recording.layer(),
                self.badge.layer(),
                self.chips.layer(),
            ])
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
            .chain([
                self.pill.layer_mut(),
                self.activity.layer_mut(),
                self.recording.layer_mut(),
                self.badge.layer_mut(),
                self.chips.layer_mut(),
            ])
            .chain(self.ripples.layers_mut().iter_mut())
            .chain(self.trail.layers_mut().iter_mut())
            .chain(self.scan.layers_mut())
            .chain(self.viewfinder.layers_mut())
    }
}
