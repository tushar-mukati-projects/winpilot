use windows::Foundation::Rect as WinRect;
use windows::Media::Ocr::{OcrLine as WinOcrLine, OcrResult};
use winpilot_core::{
    Context, Point, Rect, Result,
    ocr::{OcrLine, OcrWord},
};

use crate::frame::Frame;
use crate::resize::Fit;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Scaling {
    pub origin: Point,
    pub x: f64,
    pub y: f64,
}

impl Scaling {
    pub(super) fn between(frame: &Frame, fitted: Fit) -> Self {
        Self {
            origin: frame.origin,
            x: f64::from(frame.width) / f64::from(fitted.width),
            y: f64::from(frame.height) / f64::from(fitted.height),
        }
    }
}

pub(super) fn collect_lines(result: &OcrResult, scaling: Scaling) -> Result<Vec<OcrLine>> {
    let lines = result.Lines().context("reading OCR lines")?;
    let count = lines.Size().context("counting OCR lines")?;
    let mut collected = Vec::with_capacity(count as usize);
    for index in 0..count {
        let line = lines.GetAt(index).context("reading an OCR line")?;
        collected.push(convert_line(&line, scaling)?);
    }
    Ok(collected)
}

fn convert_line(line: &WinOcrLine, scaling: Scaling) -> Result<OcrLine> {
    let text = line.Text().context("reading OCR line text")?.to_string_lossy();
    let found = line.Words().context("reading OCR words")?;
    let mut boxes = Vec::new();
    let mut words = Vec::new();
    for index in 0..found.Size().context("counting OCR words")? {
        let word = found.GetAt(index).context("reading an OCR word")?;
        let rect = word.BoundingRect().context("reading a word box")?;
        let text = word.Text().context("reading OCR word text")?.to_string_lossy();
        words.push(OcrWord { text, rect: union_to_screen(&[rect], scaling) });
        boxes.push(rect);
    }
    Ok(OcrLine { text, rect: union_to_screen(&boxes, scaling), words })
}

fn union_to_screen(boxes: &[WinRect], scaling: Scaling) -> Rect {
    let Some(union) = union_of(boxes) else {
        return Rect::new(scaling.origin.x, scaling.origin.y, 0, 0);
    };
    let left = floor_to_screen(union.X, scaling.x, scaling.origin.x);
    let top = floor_to_screen(union.Y, scaling.y, scaling.origin.y);
    let right = ceil_to_screen(union.X + union.Width, scaling.x, scaling.origin.x);
    let bottom = ceil_to_screen(union.Y + union.Height, scaling.y, scaling.origin.y);
    Rect::new(left as i32, top as i32, (right - left) as i32, (bottom - top) as i32)
}

fn union_of(boxes: &[WinRect]) -> Option<WinRect> {
    boxes.iter().copied().reduce(|union, word| {
        let left = union.X.min(word.X);
        let top = union.Y.min(word.Y);
        let right = (union.X + union.Width).max(word.X + word.Width);
        let bottom = (union.Y + union.Height).max(word.Y + word.Height);
        WinRect { X: left, Y: top, Width: right - left, Height: bottom - top }
    })
}

fn floor_to_screen(image: f32, factor: f64, origin: i32) -> f64 {
    f64::from(origin) + (f64::from(image) * factor).floor()
}

fn ceil_to_screen(image: f32, factor: f64, origin: i32) -> f64 {
    f64::from(origin) + (f64::from(image) * factor).ceil()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_boxes_are_unioned_and_scaled() {
        let boxes = [
            WinRect { X: 10.0, Y: 5.0, Width: 20.0, Height: 10.0 },
            WinRect { X: 40.5, Y: 4.0, Width: 10.0, Height: 12.0 },
        ];
        let scaling = Scaling { origin: Point::new(-100, 200), x: 2.0, y: 2.0 };
        assert_eq!(union_to_screen(&boxes, scaling), Rect::new(-80, 208, 81, 24));
        assert_eq!(union_to_screen(&[], scaling), Rect::new(-100, 200, 0, 0));
    }

    #[test]
    fn scaling_follows_each_axis_of_the_fit() {
        let frame = Frame::black(Point::new(-5, 9), 300, 200);
        let scaling = Scaling::between(&frame, Fit { width: 150, height: 99, scale: 2.0 });
        assert_eq!(scaling.origin, Point::new(-5, 9));
        assert!((scaling.x - 2.0).abs() < 1e-12 && (scaling.y - 200.0 / 99.0).abs() < 1e-12);
    }
}
