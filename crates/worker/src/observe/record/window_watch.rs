use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use deckhand_capture::{CoverHold, Frame, Redact};
use deckhand_core::{WindowRef, config::GuardConfig};
use deckhand_desktop::WindowLabel;

use crate::blackout;

pub const DENIED_NOTE: &str =
    "parts of the recording are black because the window's title or process matched Deckhand's deny list";

pub struct WindowWatch {
    window: WindowRef,
    guard: GuardConfig,
    process: Option<(u32, String)>,
    hold: CoverHold,
    covered: bool,
    ever_denied: Arc<AtomicBool>,
}

impl WindowWatch {
    pub fn new(window: WindowRef, guard: GuardConfig) -> Self {
        Self {
            window,
            guard,
            process: None,
            hold: CoverHold::default(),
            covered: false,
            ever_denied: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn ever_denied(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.ever_denied)
    }

    fn update(&mut self, label: Option<WindowLabel>, now: Instant) -> bool {
        let Some(label) = label else { return false };
        let process = self.process_name(label.pid);
        let denied = self.guard.denies_window(&process, &label.title);
        if denied {
            self.ever_denied.store(true, Ordering::Release);
        }
        let covered = self.hold.whole_frame(denied, now);
        let changed = covered != self.covered;
        self.covered = covered;
        changed
    }

    fn process_name(&mut self, pid: u32) -> String {
        match &self.process {
            Some((known, name)) if *known == pid => name.clone(),
            _ => {
                let name = deckhand_desktop::process_name(pid);
                self.process = Some((pid, name.clone()));
                name
            }
        }
    }
}

impl Redact for WindowWatch {
    fn refresh(&mut self) -> bool {
        self.update(deckhand_desktop::window_label(self.window), Instant::now())
    }

    fn cover(&self, frame: &mut Frame) {
        if self.covered {
            let whole = frame.rect();
            blackout::fill_black(frame, &[whole]);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use deckhand_capture::{COVER_HOLD, FrameSource};
    use deckhand_core::{Point, config::DenyRule};

    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

    fn white() -> Frame {
        Frame { origin: Point::new(0, 0), width: 4, height: 2, bgra: vec![255; 32], source: FrameSource::Window }
    }

    fn label(title: &str) -> WindowLabel {
        WindowLabel { pid: 4242, title: title.to_string() }
    }

    fn watch_with_process(process: &str) -> WindowWatch {
        let mut watch = WindowWatch::new(WindowRef(1), GuardConfig::default());
        watch.process = Some((4242, process.to_string()));
        watch
    }

    #[test]
    fn a_window_whose_title_becomes_denied_is_blacked_out_and_remembered() {
        let start = Instant::now();
        let mut watch = watch_with_process("chrome.exe");
        watch.guard.deny.push(DenyRule { process: None, title: Some("Bank".to_string()) });
        assert!(!watch.update(Some(label("News")), start));
        let mut shown = white();
        watch.cover(&mut shown);
        assert_eq!(shown.bgra, vec![255; 32]);
        assert!(watch.update(Some(label("My Bank - Chrome")), start + MS(33)));
        let mut hidden = white();
        watch.cover(&mut hidden);
        assert!(hidden.bgra.as_chunks::<4>().0.iter().all(|pixel| *pixel == [0, 0, 0, 255]));
        assert!(!watch.update(Some(label("News")), start + MS(66)));
        assert!(watch.covered);
        assert!(watch.update(Some(label("News")), start + MS(33) + COVER_HOLD));
        assert!(watch.ever_denied().load(Ordering::Acquire));
    }

    #[test]
    fn a_denied_process_is_blacked_out_from_the_first_tick() {
        let start = Instant::now();
        let mut watch = watch_with_process("KeePass.exe");
        assert!(watch.update(Some(label("Database")), start));
        assert!(!watch.update(Some(label("Database")), start + MS(33)));
        assert!(watch.ever_denied().load(Ordering::Acquire));
    }

    #[test]
    fn an_unreadable_window_keeps_the_last_state() {
        let start = Instant::now();
        let mut watch = watch_with_process("KeePass.exe");
        assert!(watch.update(Some(label("Database")), start));
        assert!(!watch.update(None, start + COVER_HOLD + MS(1)));
        assert!(watch.covered);
    }
}
