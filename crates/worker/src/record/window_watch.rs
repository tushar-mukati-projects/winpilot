use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use deckhand_capture::{Frame, Redact};
use deckhand_core::{WindowRef, config::GuardConfig};
use deckhand_desktop::WindowLabel;

use crate::blackout;

pub const DENIED_NOTE: &str =
    "parts of the recording are black because the window's title or process matched Deckhand's deny list";

pub struct WindowWatch {
    window: WindowRef,
    guard: GuardConfig,
    process: Option<(u32, String)>,
    denied: bool,
    ever_denied: Arc<AtomicBool>,
}

impl WindowWatch {
    pub fn new(window: WindowRef, guard: GuardConfig) -> Self {
        Self { window, guard, process: None, denied: false, ever_denied: Arc::new(AtomicBool::new(false)) }
    }

    pub fn ever_denied(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.ever_denied)
    }

    fn update(&mut self, label: Option<WindowLabel>) -> bool {
        let Some(label) = label else { return false };
        let process = self.process_name(label.pid);
        let denied = self.guard.denies_window(&process, &label.title);
        let changed = denied != self.denied;
        self.denied = denied;
        if denied {
            self.ever_denied.store(true, Ordering::Release);
        }
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
        self.update(deckhand_desktop::window_label(self.window))
    }

    fn cover(&self, frame: &mut Frame) {
        if self.denied {
            let whole = frame.rect();
            blackout::fill_black(frame, &[whole]);
        }
    }
}

#[cfg(test)]
mod tests {
    use deckhand_capture::FrameSource;
    use deckhand_core::{Point, config::DenyRule};

    use super::*;

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
        let mut watch = watch_with_process("chrome.exe");
        watch.guard.deny.push(DenyRule { process: None, title: Some("Bank".to_string()) });
        assert!(!watch.update(Some(label("News"))));
        let mut shown = white();
        watch.cover(&mut shown);
        assert_eq!(shown.bgra, vec![255; 32]);
        assert!(watch.update(Some(label("My Bank - Chrome"))));
        let mut hidden = white();
        watch.cover(&mut hidden);
        assert!(hidden.bgra.as_chunks::<4>().0.iter().all(|pixel| *pixel == [0, 0, 0, 255]));
        assert!(watch.update(Some(label("News"))));
        assert!(watch.ever_denied().load(Ordering::Acquire));
    }

    #[test]
    fn a_denied_process_is_blacked_out_from_the_first_tick() {
        let mut watch = watch_with_process("KeePass.exe");
        assert!(watch.update(Some(label("Database"))));
        assert!(!watch.update(Some(label("Database"))));
        assert!(watch.ever_denied().load(Ordering::Acquire));
    }

    #[test]
    fn an_unreadable_window_keeps_the_last_state() {
        let mut watch = watch_with_process("KeePass.exe");
        assert!(watch.update(Some(label("Database"))));
        assert!(!watch.update(None));
        assert!(watch.denied);
    }
}
