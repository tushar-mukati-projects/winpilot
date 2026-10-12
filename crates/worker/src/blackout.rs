use deckhand_capture::Frame;
use deckhand_core::{Rect, WindowRef};
use deckhand_desktop::VisibleWindow;

const BLACK: [u8; 4] = [0, 0, 0, 255];
const BYTES_PER_PIXEL: usize = 4;

pub(crate) fn covering_rects(
    windows: &[VisibleWindow],
    denies: impl Fn(&VisibleWindow) -> bool,
    above: Option<WindowRef>,
) -> Vec<Rect> {
    windows
        .iter()
        .take_while(|window| above != Some(window.hwnd))
        .filter(|window| denies(window))
        .map(|window| window.rect)
        .collect()
}

pub(crate) fn fill_black(frame: &mut Frame, rects: &[Rect]) {
    let bounds = frame.rect();
    for rect in rects {
        if let Some(area) = bounds.intersection(*rect) {
            fill_area(frame, area);
        }
    }
}

fn fill_area(frame: &mut Frame, area: Rect) {
    let stride = frame.width as usize * BYTES_PER_PIXEL;
    let left = (area.x - frame.origin.x) as usize * BYTES_PER_PIXEL;
    let span = area.width as usize * BYTES_PER_PIXEL;
    for row in (area.y - frame.origin.y)..(area.bottom() - frame.origin.y) {
        let start = row as usize * stride + left;
        let Some(pixels) = frame.bgra.get_mut(start..start + span) else { return };
        for pixel in pixels.as_chunks_mut::<BYTES_PER_PIXEL>().0 {
            *pixel = BLACK;
        }
    }
}

#[cfg(test)]
mod tests {
    use deckhand_capture::FrameSource;
    use deckhand_core::Point;

    use super::*;

    const WHITE: [u8; 4] = [255, 255, 255, 255];

    fn white(origin: Point, width: u32, height: u32) -> Frame {
        Frame {
            origin,
            width,
            height,
            bgra: WHITE.repeat(width as usize * height as usize),
            source: FrameSource::Screen,
        }
    }

    fn visible(hwnd: i64, process: &str, title: &str, rect: Rect) -> VisibleWindow {
        VisibleWindow {
            hwnd: WindowRef(hwnd),
            pid: 1,
            process: process.to_string(),
            title: title.to_string(),
            class_name: String::new(),
            rect,
        }
    }

    fn pixel(frame: &Frame, x: i32, y: i32) -> [u8; 4] {
        let index = ((y - frame.origin.y) as usize * frame.width as usize + (x - frame.origin.x) as usize) * 4;
        frame.bgra[index..index + 4].try_into().unwrap()
    }

    fn black_count(frame: &Frame) -> usize {
        frame.bgra.as_chunks::<4>().0.iter().filter(|pixel| **pixel == BLACK).count()
    }

    #[test]
    fn fills_only_the_overlap_in_screen_coordinates() {
        let mut frame = white(Point::new(100, 50), 40, 30);
        fill_black(&mut frame, &[Rect::new(130, 70, 50, 50)]);
        assert_eq!(black_count(&frame), 10 * 10);
        assert_eq!(pixel(&frame, 130, 70), BLACK);
        assert_eq!(pixel(&frame, 139, 79), BLACK);
        assert_eq!(pixel(&frame, 129, 70), WHITE);
        assert_eq!(pixel(&frame, 130, 69), WHITE);
    }

    #[test]
    fn handles_negative_origins_and_full_cover() {
        let mut frame = white(Point::new(-1920, -100), 20, 10);
        fill_black(&mut frame, &[Rect::new(-5000, -5000, 10_000, 10_000)]);
        assert_eq!(black_count(&frame), 200);
    }

    #[test]
    fn ignores_rects_outside_the_frame() {
        let mut frame = white(Point::new(0, 0), 10, 10);
        fill_black(&mut frame, &[Rect::new(10, 0, 5, 5), Rect::new(0, -5, 5, 5), Rect::new(2, 2, 0, 4)]);
        assert_eq!(black_count(&frame), 0);
    }

    #[test]
    fn a_malformed_frame_is_left_alone() {
        let mut frame =
            Frame { origin: Point::new(0, 0), width: 10, height: 10, bgra: vec![255; 12], source: FrameSource::Screen };
        fill_black(&mut frame, &[Rect::new(0, 0, 10, 10)]);
        assert_eq!(frame.bgra, vec![255; 12]);
    }

    #[test]
    fn covers_denied_windows_including_untitled_tool_windows() {
        let windows = [
            visible(1, "keepass.exe", "Vault", Rect::new(0, 0, 100, 100)),
            visible(2, "notepad.exe", "Notes", Rect::new(50, 50, 100, 100)),
            visible(3, "keepass.exe", "", Rect::new(200, 0, 20, 20)),
        ];
        let rects = covering_rects(&windows, |window| window.process == "keepass.exe", None);
        assert_eq!(rects, [Rect::new(0, 0, 100, 100), Rect::new(200, 0, 20, 20)]);
    }

    #[test]
    fn only_windows_above_the_captured_one_are_covered() {
        let windows = [
            visible(1, "keepass.exe", "Top", Rect::new(0, 0, 10, 10)),
            visible(2, "notepad.exe", "Captured", Rect::new(0, 0, 50, 50)),
            visible(3, "keepass.exe", "Below", Rect::new(5, 5, 10, 10)),
        ];
        let denied = |window: &VisibleWindow| window.process == "keepass.exe";
        assert_eq!(covering_rects(&windows, denied, Some(WindowRef(2))), [Rect::new(0, 0, 10, 10)]);
        assert_eq!(covering_rects(&windows, denied, Some(WindowRef(99))).len(), 2);
    }
}
