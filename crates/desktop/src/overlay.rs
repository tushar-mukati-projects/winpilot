use std::{
    sync::mpsc::{self, Sender},
    time::Duration,
};

use deckhand_core::{Point, Rect, Result};

use crate::{
    keycaps::ChipText,
    message_thread::MessageThread,
    overlay_command::OverlayCommand,
    overlay_thread::{OverlaySettings, OverlayThread, WAKE_OVERLAY},
};

const CONCEAL_TIMEOUT: Duration = Duration::from_millis(250);
const NEUTRAL_LABEL: &str = "AI agent";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayOptions {
    pub animations: bool,
    pub capturable: bool,
    pub kill_hotkey: Option<String>,
    pub label: String,
    pub palette: Vec<[u8; 3]>,
}

impl Default for OverlayOptions {
    fn default() -> Self {
        Self {
            animations: true,
            capturable: false,
            kill_hotkey: None,
            label: NEUTRAL_LABEL.to_string(),
            palette: Vec::new(),
        }
    }
}

pub struct Overlay {
    commands: Sender<OverlayCommand>,
    thread: MessageThread,
    capture_excluded: bool,
    capturable: bool,
}

#[must_use = "the overlay comes back as soon as the guard is dropped"]
pub struct CaptureHideGuard<'a> {
    overlay: Option<&'a Overlay>,
}

impl CaptureHideGuard<'_> {
    pub fn is_hiding(&self) -> bool {
        self.overlay.is_some()
    }
}

impl Drop for CaptureHideGuard<'_> {
    fn drop(&mut self) {
        if let Some(overlay) = self.overlay {
            overlay.send(OverlayCommand::Unconceal);
        }
    }
}

impl Overlay {
    pub fn spawn(options: OverlayOptions) -> Result<Self> {
        let (commands, receiver) = mpsc::channel();
        let OverlayOptions { animations, capturable, kill_hotkey, label, palette } = options;
        let settings = OverlaySettings { animations, capturable, kill_hotkey, label, palette };
        let (thread, capture_excluded) =
            MessageThread::spawn("deckhand-overlay", move || OverlayThread::create(receiver, settings))?;
        Ok(Self { commands, thread, capture_excluded, capturable })
    }

    pub fn begin(&self, rect: Rect) {
        self.send(OverlayCommand::Begin(rect));
    }

    pub fn end(&self) {
        self.send(OverlayCommand::End);
    }

    pub fn click(&self, point: Point, count: u8) {
        self.send(OverlayCommand::Click(point, count));
    }

    pub fn trail(&self, from: Point, to: Point) {
        self.send(OverlayCommand::Trail(from, to));
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

    pub fn keystrokes(&self, rect: Rect, chords: Vec<Vec<String>>) {
        if !chords.is_empty() {
            self.send(OverlayCommand::Keystrokes(rect, ChipText::Keys(chords)));
        }
    }

    pub fn typing(&self, rect: Rect, characters: usize) {
        self.send(OverlayCommand::Keystrokes(rect, ChipText::Typing(characters)));
    }

    pub fn scan(&self, rect: Rect) {
        self.send(OverlayCommand::Scan(rect));
    }

    pub fn show_target(&self, rect: Rect) {
        self.begin(rect);
    }

    pub fn flash_point(&self, point: Point) {
        self.click(point, 1);
    }

    pub fn hide(&self) {
        self.end();
    }

    pub fn capture_excluded(&self) -> bool {
        self.capture_excluded
    }

    pub fn hidden_for_capture(&self) -> CaptureHideGuard<'_> {
        if !self.capturable {
            return CaptureHideGuard { overlay: None };
        }
        let (confirm, confirmed) = mpsc::channel();
        if !self.send(OverlayCommand::Conceal(confirm)) {
            return CaptureHideGuard { overlay: None };
        }
        let _confirmed = confirmed.recv_timeout(CONCEAL_TIMEOUT);
        CaptureHideGuard { overlay: Some(self) }
    }

    #[cfg(test)]
    pub(crate) fn thread_id(&self) -> u32 {
        self.thread.thread_id()
    }

    fn send(&self, command: OverlayCommand) -> bool {
        self.commands.send(command).is_ok() && self.thread.post(WAKE_OVERLAY)
    }
}
