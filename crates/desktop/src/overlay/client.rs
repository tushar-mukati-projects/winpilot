use std::{
    sync::mpsc::{self, Receiver, Sender},
    time::Duration,
};

use winpilot_core::{Point, Rect, Result};

use super::{
    activity::Activity,
    capture_hide::CaptureHideGuard,
    chip_text::ChipText,
    command::OverlayCommand,
    options::OverlayOptions,
    presence::Driving,
    render_thread::{OverlayThread, WAKE_OVERLAY},
    wake_flag::WakeFlag,
};
use winpilot_win32::MessageThread;

const CONCEAL_TIMEOUT: Duration = Duration::from_millis(250);

pub struct Overlay {
    commands: Sender<OverlayCommand>,
    wake: WakeFlag,
    thread: MessageThread,
    every_layer_excluded: bool,
}

impl Overlay {
    pub fn spawn(options: OverlayOptions) -> Result<Self> {
        let (commands, receiver) = mpsc::channel();
        let wake = WakeFlag::default();
        let thread_wake = wake.clone();
        let (thread, every_layer_excluded) =
            MessageThread::spawn("winpilot-overlay", move || OverlayThread::create(receiver, thread_wake, options))?;
        Ok(Self { commands, wake, thread, every_layer_excluded })
    }

    pub fn begin(&self, rect: Rect) {
        self.send(OverlayCommand::Begin(rect, Driving::Foreground));
    }

    pub fn begin_in_background(&self, rect: Rect) {
        self.send(OverlayCommand::Begin(rect, Driving::Background));
    }

    pub fn end(&self) {
        self.send(OverlayCommand::End);
    }

    pub fn click(&self, point: Point, count: u8) {
        self.send(OverlayCommand::Click(point, count));
    }

    pub fn ripple(&self, point: Point, count: u8) {
        self.send(OverlayCommand::Ripple(point, count));
    }

    pub fn background_activity(&self, activity: Activity) {
        self.send(OverlayCommand::Activity(activity));
    }

    pub fn glide(&self, from: Point, to: Point, duration: Duration) {
        self.send(OverlayCommand::Glide(from, to, duration));
    }

    pub fn focus_field(&self, rect: Rect) {
        self.send(OverlayCommand::FocusField(rect));
    }

    pub fn stopped(&self) {
        self.send(OverlayCommand::Stopped);
    }

    pub fn resumed(&self) {
        self.send(OverlayCommand::Resumed);
    }

    pub fn recording(&self, rect: Rect) {
        self.send(OverlayCommand::Recording(rect));
    }

    pub fn recording_stopped(&self) {
        self.send(OverlayCommand::RecordingStopped);
    }

    pub fn screenshot(&self, rect: Rect) {
        self.send(OverlayCommand::Screenshot(rect));
    }

    pub fn keys(&self, rect: Rect, chords: Vec<Vec<String>>) {
        let text = ChipText::keys(chords);
        if !text.is_empty() {
            self.send(OverlayCommand::Keystrokes(rect, text));
        }
    }

    pub fn typing(&self, rect: Rect) {
        self.send(OverlayCommand::Keystrokes(rect, ChipText::typing()));
    }

    pub fn relabel(&self, label: String) {
        self.send(OverlayCommand::Relabel(label));
    }

    pub fn scan(&self, rect: Rect) {
        self.send(OverlayCommand::Scan(rect));
    }

    pub fn hidden_for_capture(&self) -> CaptureHideGuard<'_> {
        if self.every_layer_excluded {
            return CaptureHideGuard::not_needed();
        }
        let (confirm, confirmed) = mpsc::channel();
        if self.commands.send(OverlayCommand::Conceal(confirm)).is_err() {
            return CaptureHideGuard::not_needed();
        }
        self.wake();
        let confirmed = waited_for(&confirmed) || (self.thread.post(WAKE_OVERLAY) && waited_for(&confirmed));
        CaptureHideGuard::hiding(self, confirmed)
    }

    pub(crate) fn unconceal(&self) {
        self.send(OverlayCommand::Unconceal);
    }

    #[cfg(test)]
    pub(crate) fn thread_id(&self) -> u32 {
        self.thread.thread_id()
    }

    fn send(&self, command: OverlayCommand) {
        if self.commands.send(command).is_ok() {
            self.wake();
        }
    }

    fn wake(&self) {
        if self.wake.claim() && !self.thread.post(WAKE_OVERLAY) {
            self.wake.give_back();
        }
    }
}

fn waited_for(confirmed: &Receiver<()>) -> bool {
    confirmed.recv_timeout(CONCEAL_TIMEOUT).is_ok()
}
