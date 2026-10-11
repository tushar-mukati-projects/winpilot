use std::{sync::mpsc::Receiver, time::Instant};

use deckhand_core::{Point, Rect, Result};
use windows::Win32::{
    Graphics::Dwm::DwmFlush,
    UI::{
        HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext},
        WindowsAndMessaging::{MSG, WM_APP},
    },
};

use crate::{
    border_geometry::{BorderMetrics, OpenSides, fit_on_screen, outside_probes},
    frame_clock::FrameClock,
    message_thread::ThreadMessages,
    monitor_at::{on_a_monitor, primary_work_area, scale_at},
    monitors::virtual_screen,
    motion::system_animations_enabled,
    overlay_command::OverlayCommand,
    overlay_scene::{Look, Scene},
    overlay_stage::{Cast, Stage},
    overlay_window::{Capture, ensure_classes_registered, module_instance},
    palette::Palette,
    pill_words::hotkey_label,
};

pub(crate) const WAKE_OVERLAY: u32 = WM_APP + 0x0D;
const COMPOSITIONS_TO_WAIT: usize = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OverlaySettings {
    pub(crate) animations: bool,
    pub(crate) capturable: bool,
    pub(crate) kill_hotkey: Option<String>,
    pub(crate) label: String,
    pub(crate) palette: Vec<[u8; 3]>,
}

pub(crate) struct OverlayThread {
    clock: FrameClock,
    stage: Stage,
    scene: Scene,
    animations: bool,
    concealed: usize,
    tick: u64,
    commands: Receiver<OverlayCommand>,
}

impl OverlayThread {
    pub(crate) fn create(commands: Receiver<OverlayCommand>, settings: OverlaySettings) -> Result<(Self, bool)> {
        let _previous_awareness = unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        let instance = module_instance()?;
        ensure_classes_registered(instance)?;
        let presence = if settings.capturable { Capture::Allowed } else { Capture::Excluded };
        let hotkey = settings.kill_hotkey.as_deref().and_then(hotkey_label);
        let cast = Cast { label: settings.label, hotkey, palette: Palette::looping(&settings.palette) };
        let (stage, applied) = Stage::create(instance, presence, cast)?;
        let thread = Self {
            clock: FrameClock::default(),
            stage,
            scene: Scene::default(),
            animations: settings.animations,
            concealed: 0,
            tick: 0,
            commands,
        };
        Ok((thread, applied && !settings.capturable))
    }

    fn apply_pending(&mut self) {
        let now = Instant::now();
        let mut raise = false;
        while let Ok(command) = self.commands.try_recv() {
            raise |= matches!(command, OverlayCommand::Begin(_) | OverlayCommand::Stopped);
            self.apply(command, now);
        }
        self.render(now);
        if raise {
            self.stage.raise();
        }
    }

    fn apply(&mut self, command: OverlayCommand, now: Instant) {
        match command {
            OverlayCommand::Begin(rect) => {
                let (frame, look) = self.framed(rect);
                self.scene.begin(frame, look, now);
            }
            OverlayCommand::End => self.scene.end(now),
            OverlayCommand::Click(point, count) => self.scene.click(point, count, self.look_at(point), now),
            OverlayCommand::Trail(from, to) => self.scene.trail(from, to, self.look_at(to), now),
            OverlayCommand::Glide(from, to, duration) => self.scene.glide(from, to, duration, self.look_at(to), now),
            OverlayCommand::FocusField(rect) => self.scene.focus_field(rect, self.look_at(top_centre(rect)), now),
            OverlayCommand::Stopped => self.stop(now),
            OverlayCommand::Resumed => self.scene.resumed(now),
            OverlayCommand::Recording(rect) => self.scene.recording(rect, self.look_at(top_centre(rect)), now),
            OverlayCommand::RecordingStopped => self.scene.recording_stopped(),
            OverlayCommand::Screenshot(rect) => self.scene.screenshot(rect, self.look_at(top_centre(rect)), now),
            OverlayCommand::Keystrokes(rect, text) => {
                self.scene.keystrokes(rect, text, self.look_at(rect.center()), now);
            }
            OverlayCommand::Scan(rect) => {
                let _shown = self.scene.scan(rect, self.look_at(top_centre(rect)), now);
            }
            OverlayCommand::Conceal(confirm) => {
                self.conceal();
                let _confirmed = confirm.send(());
            }
            OverlayCommand::Unconceal => self.concealed = self.concealed.saturating_sub(1),
        }
    }

    fn render(&mut self, now: Instant) {
        self.scene.advance(now);
        self.tick = self.tick.wrapping_add(1);
        self.stage.show(&self.scene.frame(now), virtual_screen(), self.concealed > 0, self.tick);
        if self.scene.is_empty() {
            self.stage.release_memory();
        }
        let timer_window = self.stage.timer_layer().window().handle();
        self.clock.schedule(timer_window, self.scene.next_wake(now));
    }

    fn stop(&mut self, now: Instant) {
        let screen = virtual_screen();
        let area = primary_work_area().unwrap_or(screen);
        self.scene.stopped(area, self.look_at(top_centre(area)), now);
    }

    fn conceal(&mut self) {
        self.concealed += 1;
        self.stage.reveal(true);
        for _ in 0..COMPOSITIONS_TO_WAIT {
            if unsafe { DwmFlush() }.is_err() {
                break;
            }
        }
    }

    fn framed(&self, rect: Rect) -> (Rect, Look) {
        let look = self.look_at(top_centre(rect));
        let band = BorderMetrics::at_scale(look.scale).band;
        let clipped = rect.intersection(virtual_screen()).unwrap_or(rect);
        let [left, top, right, bottom] = outside_probes(clipped, band).map(|(x, y)| on_a_monitor(Point::new(x, y)));
        (fit_on_screen(clipped, band, OpenSides { left, top, right, bottom }), look)
    }

    fn look_at(&self, point: Point) -> Look {
        Look { scale: scale_at(point), animated: self.animations && system_animations_enabled() }
    }
}

impl ThreadMessages for OverlayThread {
    fn on_message(&mut self, message: &MSG) -> bool {
        if message.message == WAKE_OVERLAY && message.hwnd.is_invalid() {
            self.apply_pending();
            return true;
        }
        if self.clock.fired(message) && self.stage.owns_timer(message) {
            self.render(Instant::now());
            return true;
        }
        false
    }
}

fn top_centre(rect: Rect) -> Point {
    Point::new(rect.center().x, rect.y)
}
