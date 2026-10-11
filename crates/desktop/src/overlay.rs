use std::sync::mpsc::{self, Sender};

use deckhand_core::{Point, Rect, Result};

use crate::{
    message_thread::MessageThread,
    overlay_window::{OverlayCommand, OverlayWindows, WAKE_OVERLAY},
};

pub struct Overlay {
    commands: Sender<OverlayCommand>,
    thread: MessageThread,
    capture_excluded: bool,
}

impl Overlay {
    pub fn spawn() -> Result<Self> {
        let (commands, receiver) = mpsc::channel();
        let (thread, status) = MessageThread::spawn("deckhand-overlay", move || OverlayWindows::create(receiver))?;
        Ok(Self { commands, thread, capture_excluded: status.capture_excluded })
    }

    pub fn show_target(&self, rect: Rect) {
        self.send(OverlayCommand::ShowTarget(rect));
    }

    pub fn flash_point(&self, point: Point) {
        self.send(OverlayCommand::FlashPoint(point));
    }

    pub fn hide(&self) {
        self.send(OverlayCommand::Hide);
    }

    pub fn capture_excluded(&self) -> bool {
        self.capture_excluded
    }

    fn send(&self, command: OverlayCommand) {
        if self.commands.send(command).is_ok() {
            self.thread.post(WAKE_OVERLAY);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn overlay_is_send_and_sync() {
        assert_send_sync::<Overlay>();
    }

    #[test]
    #[ignore = "shows the overlay on screen"]
    fn overlay_shows_border_and_ripple_then_hides() {
        let overlay = Overlay::spawn().unwrap();
        assert!(overlay.capture_excluded());
        overlay.show_target(Rect::new(200, 200, 600, 400));
        overlay.flash_point(Point::new(500, 400));
        thread::sleep(Duration::from_millis(800));
        overlay.hide();
        thread::sleep(Duration::from_millis(100));
    }
}
