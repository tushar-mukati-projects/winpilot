use windows::Win32::Graphics::Gdi::{BitBlt, SRCCOPY};
use winpilot_core::{Context, Error, Point, Rect, Result};

use crate::frame::Frame;
use crate::gdi::{GdiSurface, ScreenDc};
use crate::pixels::{Block, copy_block};

#[derive(Default)]
pub(crate) struct ScreenGrabber {
    surface: Option<GdiSurface>,
}

impl ScreenGrabber {
    pub(crate) fn grab_into(&mut self, part: Rect, frame: &mut Frame) -> Result<()> {
        let (width, height) = (part.width as u32, part.height as u32);
        let surface = self.surface_holding(width, height)?;
        let screen = ScreenDc::acquire()?;
        unsafe { BitBlt(surface.dc(), 0, 0, part.width, part.height, Some(screen.dc()), part.x, part.y, SRCCOPY) }
            .context("BitBlt from the screen")?;
        let from = Block { x: 0, y: 0, width, height };
        let to = frame.offset_of(Point::new(part.x, part.y));
        copy_block(&surface.pixels()?, from, &mut frame.pixels_mut(), to)
    }

    fn surface_holding(&mut self, width: u32, height: u32) -> Result<&GdiSurface> {
        if !self.surface.as_ref().is_some_and(|surface| surface.holds(width, height)) {
            let (held_width, held_height) = self.surface.take().map_or((0, 0), |surface| surface.size());
            self.surface = Some(GdiSurface::new(held_width.max(width), held_height.max(height))?);
        }
        self.surface.as_ref().ok_or_else(|| Error::internal("the screen surface was not created"))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use super::*;

    fn primary_monitor() -> Rect {
        crate::dpi::make_thread_per_monitor_aware().unwrap();
        let monitors = crate::monitor_list::enumerate_monitors().unwrap();
        monitors.iter().find(|monitor| monitor.primary).unwrap().rect
    }

    fn best_and_median_ms(grabber: &mut ScreenGrabber, part: Rect) -> (f64, f64) {
        let mut frame = Frame::black(Point::new(part.x, part.y), part.width as u32, part.height as u32);
        let mut timings: Vec<f64> = (0..15)
            .map(|_| {
                let started = Instant::now();
                grabber.grab_into(part, &mut frame).unwrap();
                started.elapsed().as_secs_f64() * 1000.0
            })
            .collect();
        timings.sort_by(f64::total_cmp);
        (timings[0], timings[timings.len() / 2])
    }

    #[test]
    fn the_surface_is_reused_and_only_grows() {
        crate::dpi::make_thread_per_monitor_aware().unwrap();
        let mut grabber = ScreenGrabber::default();
        let mut frame = Frame::black(Point::new(0, 0), 64, 32);
        grabber.grab_into(Rect::new(0, 0, 64, 32), &mut frame).unwrap();
        let first = grabber.surface.as_ref().unwrap().dc();
        grabber.grab_into(Rect::new(0, 0, 16, 8), &mut frame).unwrap();
        assert_eq!(grabber.surface.as_ref().unwrap().dc(), first);
        assert_eq!(grabber.surface.as_ref().unwrap().size(), (64, 32));
        let mut tall = Frame::black(Point::new(0, 0), 8, 40);
        grabber.grab_into(Rect::new(0, 0, 8, 40), &mut tall).unwrap();
        assert_eq!(grabber.surface.as_ref().unwrap().size(), (64, 40));
    }

    #[test]
    fn grabs_land_at_their_offset_in_the_frame() {
        crate::dpi::make_thread_per_monitor_aware().unwrap();
        let mut grabber = ScreenGrabber::default();
        let mut frame = Frame::black(Point::new(-10, -10), 30, 30);
        grabber.grab_into(Rect::new(0, 0, 20, 20), &mut frame).unwrap();
        let untouched = frame.crop(Rect::new(-10, -10, 10, 30)).unwrap();
        assert!(untouched.bgra.iter().all(|&byte| byte == 0));
    }

    #[test]
    fn gdi_full_monitor_and_region_timing() {
        let monitor = primary_monitor();
        let region = Rect::new(monitor.x, monitor.y, monitor.width / 2, monitor.height / 2);
        let mut grabber = ScreenGrabber::default();
        let (best, median) = best_and_median_ms(&mut grabber, monitor);
        let (width, height) = (monitor.width, monitor.height);
        eprintln!("timing: gdi grab {width}x{height} full monitor: best {best:.2} ms, median {median:.2} ms");
        let (best, median) = best_and_median_ms(&mut grabber, region);
        let (width, height) = (region.width, region.height);
        eprintln!("timing: gdi grab {width}x{height} region: best {best:.2} ms, median {median:.2} ms");
    }
}
