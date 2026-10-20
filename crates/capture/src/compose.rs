use winpilot_core::{Point, Result};

use crate::capture_area::CaptureArea;
use crate::frame::Frame;
use crate::output::Output;
use crate::screen_grabber::ScreenGrabber;

pub(crate) fn capture_area(outputs: &mut [Output], grabber: &mut ScreenGrabber, area: CaptureArea) -> Result<Frame> {
    let rect = area.rect();
    let mut frame = Frame::black(Point::new(rect.x, rect.y), rect.width as u32, rect.height as u32);
    for output in outputs.iter_mut() {
        let Some(part) = output.rect().intersection(rect) else {
            continue;
        };
        output.capture_into(part, &mut frame, grabber)?;
    }
    Ok(frame)
}
