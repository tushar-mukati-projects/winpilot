use std::time::Instant;

use winpilot_capture::{CoverHold, Frame, Redact};
use winpilot_core::{Rect, Result, config::GuardConfig};
use winpilot_desktop::VisibleWindow;

use crate::blackout;

pub(crate) struct DeniedWindows {
    guard: GuardConfig,
    hold: CoverHold,
    rects: Vec<Rect>,
    whole_frame: bool,
}

impl DeniedWindows {
    pub(crate) fn new(guard: GuardConfig) -> Self {
        Self { guard, hold: CoverHold::default(), rects: Vec::new(), whole_frame: true }
    }

    fn update(&mut self, windows: Result<Vec<VisibleWindow>>, now: Instant) -> bool {
        let guard = &self.guard;
        let (current, unknown) = match windows {
            Ok(windows) => (
                blackout::covering_rects(&windows, |window| guard.denies_window(&window.process, &window.title), None),
                false,
            ),
            Err(_) => (Vec::new(), true),
        };
        let rects = self.hold.rects(&current, now);
        let whole_frame = self.hold.whole_frame(unknown, now);
        let changed = rects != self.rects || whole_frame != self.whole_frame;
        self.rects = rects;
        self.whole_frame = whole_frame;
        changed
    }
}

impl Redact for DeniedWindows {
    fn refresh(&mut self) -> bool {
        self.update(winpilot_desktop::all_visible_windows(), Instant::now())
    }

    fn cover(&self, frame: &mut Frame) {
        if self.whole_frame {
            let whole = frame.rect();
            blackout::fill_black(frame, &[whole]);
            return;
        }
        blackout::fill_black(frame, &self.rects);
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use winpilot_capture::{COVER_HOLD, FrameSource};
    use winpilot_core::{Error, Point, WindowRef};

    use super::*;

    const MS: fn(u64) -> Duration = Duration::from_millis;

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
        assert!(denied.update(Ok(windows), Instant::now()));
        let mut frame = white(Point::new(100, 50), 40, 30);
        denied.cover(&mut frame);
        assert_eq!(black_pixels(&frame), 20);
    }

    #[test]
    fn an_unreadable_window_list_blacks_out_the_whole_frame() {
        let start = Instant::now();
        let mut denied = DeniedWindows::new(GuardConfig::default());
        let mut untouched = white(Point::new(0, 0), 8, 8);
        denied.cover(&mut untouched);
        assert_eq!(black_pixels(&untouched), 64);
        assert!(!denied.update(Err(Error::internal("enumeration failed")), start));
        let mut frame = white(Point::new(0, 0), 8, 8);
        denied.cover(&mut frame);
        assert_eq!(black_pixels(&frame), 64);
        assert!(!denied.update(Ok(Vec::new()), start + MS(100)));
        assert!(denied.update(Ok(Vec::new()), start + COVER_HOLD));
        let mut clear = white(Point::new(0, 0), 8, 8);
        denied.cover(&mut clear);
        assert_eq!(black_pixels(&clear), 0);
    }

    #[test]
    fn every_refresh_reports_whether_the_denied_areas_changed() {
        let start = Instant::now();
        let mut denied = DeniedWindows::new(GuardConfig::default());
        let vault = || vec![visible("KeePass.exe", Rect::new(0, 0, 10, 10))];
        assert!(denied.update(Ok(vault()), start));
        assert!(!denied.update(Ok(vault()), start + MS(33)));
        assert!(denied.update(Ok(vec![visible("KeePass.exe", Rect::new(5, 0, 10, 10))]), start + MS(66)));
        assert_eq!(denied.rects, [Rect::new(0, 0, 10, 10), Rect::new(5, 0, 10, 10)]);
        assert!(denied.update(Ok(Vec::new()), start + MS(66) + COVER_HOLD));
        assert!(!denied.update(Ok(vec![visible("notepad.exe", Rect::new(0, 0, 10, 10))]), start + MS(1000)));
        assert!(denied.rects.is_empty());
    }

    #[test]
    fn a_denied_window_that_minimizes_stays_black_for_the_hold() {
        let start = Instant::now();
        let mut denied = DeniedWindows::new(GuardConfig::default());
        let vault = Rect::new(0, 0, 4, 4);
        assert!(denied.update(Ok(vec![visible("KeePass.exe", vault)]), start));
        assert!(!denied.update(Ok(Vec::new()), start + MS(50)));
        let mut frame = white(Point::new(0, 0), 4, 4);
        denied.cover(&mut frame);
        assert_eq!(black_pixels(&frame), 16);
        assert!(denied.update(Ok(Vec::new()), start + COVER_HOLD + MS(50)));
    }

    #[test]
    fn the_live_redactor_runs_against_the_real_window_list() {
        let mut redactor = DeniedWindows::new(GuardConfig::default());
        let started = Instant::now();
        assert!(redactor.refresh());
        let elapsed = started.elapsed();
        let mut frame = white(Point::new(-90_000, -90_000), 4, 4);
        redactor.cover(&mut frame);
        assert_eq!(black_pixels(&frame), 0);
        eprintln!("timing: denied-window refresh took {:.2} ms", elapsed.as_secs_f64() * 1000.0);
    }
}
