use std::{sync::mpsc::Receiver, time::Instant};

use windows::Win32::{
    Graphics::Dwm::DwmFlush,
    UI::{
        HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext},
        WindowsAndMessaging::{MSG, WM_APP},
    },
};
use winpilot_core::{Point, Rect, Result};

use super::{
    activity::Activity,
    activity_words::activity_hint,
    border_geometry::{BorderMetrics, fit_on_screen, is_framable, sides_open_within},
    command::OverlayCommand,
    driving::Driving,
    frame_clock::FrameClock,
    monitor_at::{foreground_work_area, monitor_around, primary_work_area, scale_at},
    motion::system_animations_enabled,
    options::OverlayOptions,
    palette::Palette,
    pill_words::hotkey_label,
    scene::Scene,
    scene_items::Look,
    stage::{Cast, Stage},
    wake_flag::WakeFlag,
    window::{Capture, ensure_classes_registered},
};
use crate::{message_thread::ThreadMessages, module_instance::module_instance, monitors::virtual_screen};

pub(crate) const WAKE_OVERLAY: u32 = WM_APP + 0x0D;
const COMPOSITIONS_TO_WAIT: usize = 2;

pub(crate) struct OverlayThread {
    clock: FrameClock,
    stage: Stage,
    scene: Scene,
    animations: bool,
    concealed: usize,
    tick: u64,
    commands: Receiver<OverlayCommand>,
    wake: WakeFlag,
}

impl OverlayThread {
    pub(crate) fn create(
        commands: Receiver<OverlayCommand>,
        wake: WakeFlag,
        settings: OverlayOptions,
    ) -> Result<(Self, bool)> {
        let _previous_awareness = unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        let instance = module_instance()?;
        ensure_classes_registered(instance)?;
        let presence = if settings.capturable { Capture::Allowed } else { Capture::Excluded };
        let hotkey = settings.kill_hotkey.as_deref().and_then(hotkey_label);
        let cast = Cast { label: settings.label, hotkey, palette: Palette::looping(&settings.palette) };
        let stage = Stage::create(instance, presence, cast)?;
        let every_layer_excluded = stage.every_layer_excluded_from_capture();
        let thread = Self {
            clock: FrameClock::default(),
            stage,
            scene: Scene::default(),
            animations: settings.animations,
            concealed: 0,
            tick: 0,
            commands,
            wake,
        };
        Ok((thread, every_layer_excluded))
    }

    fn apply_pending(&mut self) {
        self.wake.take();
        let now = Instant::now();
        let mut raise = false;
        let mut applied = 0usize;
        while let Ok(command) = self.commands.try_recv() {
            raise |=
                matches!(command, OverlayCommand::Begin(..) | OverlayCommand::Activity(_) | OverlayCommand::Stopped);
            self.apply(command, now);
            applied += 1;
        }
        if applied == 0 {
            return;
        }
        self.render(now);
        if raise {
            self.stage.raise();
        }
    }

    fn apply(&mut self, command: OverlayCommand, now: Instant) {
        match command {
            OverlayCommand::Begin(rect, driving) => self.begin(rect, driving, now),
            OverlayCommand::End => self.scene.end(now),
            OverlayCommand::Click(point, count) => self.scene.click(point, count, self.look_at(point), now),
            OverlayCommand::Ripple(point, count) => self.scene.ripple(point, count, self.look_at(point), now),
            OverlayCommand::Activity(activity) => self.activity(&activity, now),
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
            OverlayCommand::Relabel(label) => self.stage.relabel(&label),
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

    fn begin(&mut self, rect: Rect, driving: Driving, now: Instant) {
        if is_framable(rect) {
            let (frame, look) = self.framed(rect);
            self.scene.begin(frame, look, driving, now);
        }
    }

    fn activity(&mut self, activity: &Activity, now: Instant) {
        let area = foreground_work_area().or_else(primary_work_area).unwrap_or_else(virtual_screen);
        let look = self.look_at(Point::new(area.center().x, area.bottom().saturating_sub(1)));
        self.scene.activity(area, activity_hint(activity), look, now);
    }

    fn stop(&mut self, now: Instant) {
        let screen = virtual_screen();
        let area = primary_work_area().unwrap_or(screen);
        self.scene.stopped(area, self.look_at(top_centre(area)), now);
    }

    fn conceal(&mut self) {
        self.concealed += 1;
        if self.stage.conceal() {
            wait_for_compositions();
        }
    }

    fn framed(&self, rect: Rect) -> (Rect, Look) {
        let look = self.look_at(top_centre(rect));
        let band = BorderMetrics::at_scale(look.scale).band;
        let clipped = rect.intersection(virtual_screen()).unwrap_or(rect);
        let monitor = monitor_around(clipped).map_or(clipped, |monitor| monitor.rect);
        (fit_on_screen(clipped, band, sides_open_within(clipped, band, monitor)), look)
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

fn wait_for_compositions() {
    for _ in 0..COMPOSITIONS_TO_WAIT {
        if unsafe { DwmFlush() }.is_err() {
            break;
        }
    }
}

fn top_centre(rect: Rect) -> Point {
    Point::new(rect.center().x, rect.y)
}
