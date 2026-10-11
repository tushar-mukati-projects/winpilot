use std::time::{Duration, Instant};

use deckhand_capture::{Frame, Redactor};
use deckhand_core::{Rect, Result, config::GuardConfig};
use deckhand_desktop::VisibleWindow;

use crate::blackout;

const REFRESH_EVERY: Duration = Duration::from_millis(500);

struct DeniedWindows {
    guard: GuardConfig,
    rects: Vec<Rect>,
    refreshed: Option<Instant>,
    unknown: bool,
}

pub fn redactor(guard: GuardConfig) -> Redactor {
    let mut denied = DeniedWindows::new(guard);
    Box::new(move |frame: &mut Frame| {
        if denied.is_due(Instant::now()) {
            denied.refresh(deckhand_desktop::all_visible_windows());
        }
        denied.cover(frame);
    })
}

impl DeniedWindows {
    const fn new(guard: GuardConfig) -> Self {
        Self { guard, rects: Vec::new(), refreshed: None, unknown: true }
    }

    fn is_due(&self, now: Instant) -> bool {
        self.refreshed.is_none_or(|at| now.saturating_duration_since(at) >= REFRESH_EVERY)
    }

    fn refresh(&mut self, windows: Result<Vec<VisibleWindow>>) {
        self.refreshed = Some(Instant::now());
        let Ok(windows) = windows else {
            self.unknown = true;
            return;
        };
        let guard = &self.guard;
        self.rects =
            blackout::covering_rects(&windows, |window| guard.denies_window(&window.process, &window.title), None);
        self.unknown = false;
    }

    fn cover(&self, frame: &mut Frame) {
        if self.unknown {
            let whole = frame.rect();
            blackout::fill_black(frame, &[whole]);
            return;
        }
        blackout::fill_black(frame, &self.rects);
    }
}

#[cfg(test)]
mod tests {
    use deckhand_capture::FrameSource;
    use deckhand_core::{Error, Point, WindowRef};

    use super::*;

    fn white(origin: Point, width: u32, height: u32) -> Frame {
        Frame {
            origin,
            width,
            height,
            bgra: [255, 255, 255, 255].repeat(width as usize * height as usize),
            source: FrameSource::Screen,
        }
    }

    fn visible(process: &str, rect: Rect) -> VisibleWindow {
        VisibleWindow {
            hwnd: WindowRef(1),
            pid: 1,
            process: process.to_string(),
            title: "Vault".to_string(),
            class_name: String::new(),
            rect,
        }
    }

    fn black_pixels(frame: &Frame) -> usize {
        frame.bgra.as_chunks::<4>().0.iter().filter(|pixel| **pixel == [0, 0, 0, 255]).count()
    }

    #[test]
    fn denied_windows_are_blacked_out_in_screen_coordinates() {
        let mut denied = DeniedWindows::new(GuardConfig::default());
        let windows =
            vec![visible("KeePass.exe", Rect::new(110, 60, 5, 4)), visible("notepad.exe", Rect::new(100, 50, 40, 30))];
        denied.refresh(Ok(windows));
        let mut frame = white(Point::new(100, 50), 40, 30);
        denied.cover(&mut frame);
        assert_eq!(black_pixels(&frame), 20);
    }

    #[test]
    fn an_unreadable_window_list_blacks_out_the_whole_frame() {
        let mut denied = DeniedWindows::new(GuardConfig::default());
        let mut untouched = white(Point::new(0, 0), 8, 8);
        denied.cover(&mut untouched);
        assert_eq!(black_pixels(&untouched), 64);
        denied.refresh(Err(Error::internal("enumeration failed")));
        let mut frame = white(Point::new(0, 0), 8, 8);
        denied.cover(&mut frame);
        assert_eq!(black_pixels(&frame), 64);
        denied.refresh(Ok(Vec::new()));
        let mut clear = white(Point::new(0, 0), 8, 8);
        denied.cover(&mut clear);
        assert_eq!(black_pixels(&clear), 0);
    }

    #[test]
    fn the_window_list_is_refreshed_about_twice_a_second() {
        let mut denied = DeniedWindows::new(GuardConfig::default());
        let now = Instant::now();
        assert!(denied.is_due(now));
        denied.refresh(Ok(Vec::new()));
        let refreshed = denied.refreshed.unwrap();
        assert!(!denied.is_due(refreshed + Duration::from_millis(499)));
        assert!(denied.is_due(refreshed + REFRESH_EVERY));
    }

    #[test]
    fn the_live_redactor_runs_against_the_real_window_list() {
        let mut redact = redactor(GuardConfig::default());
        let mut frame = white(Point::new(-90_000, -90_000), 4, 4);
        redact(&mut frame);
        assert_eq!(black_pixels(&frame), 0);
    }
}
