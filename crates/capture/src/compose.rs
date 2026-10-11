use deckhand_core::{Point, Rect, Result};

use crate::frame::{Frame, overlap, validate_capture_rect};
use crate::output::Output;

pub(crate) fn capture_rect(outputs: &mut [Output], rect: Rect) -> Result<Frame> {
    validate_capture_rect(rect)?;
    let mut frame = Frame::black(Point::new(rect.x, rect.y), rect.width as u32, rect.height as u32);
    for output in outputs.iter_mut() {
        let Some(part) = overlap(output.rect(), rect) else {
            continue;
        };
        output.capture_into(part, &mut frame)?;
    }
    Ok(frame)
}
