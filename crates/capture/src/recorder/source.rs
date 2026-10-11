use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use deckhand_core::{Error, Point, Rect, Result};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::IsWindow;

use super::gpu::Gpu;
use super::layout::Size;
use super::monitor_area::MonitorArea;
use super::spec::RecordingSource;
use super::staging::{Staging, texture_size};
use super::wgc::{CaptureSession, CapturedFrame, ensure_supported, monitor_item, window_item};
use crate::convert::hwnd_from_ref;
use crate::frame::{Frame, FrameSource};

const FIRST_FRAME_PATIENCE: Duration = Duration::from_secs(3);
const FIRST_FRAME_POLL: Duration = Duration::from_millis(10);
const ALIVE_CHECK_EVERY: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy)]
enum Geometry {
    Window { hwnd: HWND, crop: Option<Rect> },
    Screen(MonitorArea),
}

pub(crate) struct Source {
    pub frame: Frame,
    session: CaptureSession,
    staging: Staging,
    gpu: Gpu,
    geometry: Geometry,
    alive_checked: Instant,
}

impl Source {
    pub(crate) fn open(source: RecordingSource, cursor: bool) -> Result<(Self, bool)> {
        ensure_supported()?;
        let (geometry, clipped) = geometry_of(source)?;
        let gpu = Gpu::create()?;
        let item = match geometry {
            Geometry::Window { hwnd, .. } => window_item(hwnd)?,
            Geometry::Screen(area) => monitor_item(area.monitor)?,
        };
        let session = CaptureSession::open(&gpu, item, cursor)?;
        let frame =
            Frame { origin: Point::new(0, 0), width: 0, height: 0, bgra: Vec::new(), source: FrameSource::Screen };
        let opened = Self { frame, session, staging: Staging::default(), gpu, geometry, alive_checked: Instant::now() };
        Ok((opened, clipped))
    }

    pub(crate) fn wait_for_first_frame(&mut self, stop: &AtomicBool) -> Result<()> {
        let started = Instant::now();
        while started.elapsed() < FIRST_FRAME_PATIENCE {
            if stop.load(Ordering::Acquire) {
                return Err(Error::new(
                    deckhand_core::ErrorCode::Cancelled,
                    "the recording was stopped while starting",
                ));
            }
            if self.grab()? {
                return Ok(());
            }
            thread::sleep(FIRST_FRAME_POLL);
        }
        Err(self.no_frames())
    }

    pub(crate) const fn size(&self) -> Size {
        Size::new(self.frame.width, self.frame.height)
    }

    pub(crate) fn grab(&mut self) -> Result<bool> {
        let Some(captured) = self.session.latest() else { return Ok(false) };
        let content = self.read(&captured)?;
        drop(captured);
        self.session.follow_size(&self.gpu, content)?;
        Ok(true)
    }

    pub(crate) fn is_gone(&mut self) -> bool {
        let Geometry::Window { hwnd, .. } = self.geometry else { return false };
        if self.alive_checked.elapsed() < ALIVE_CHECK_EVERY {
            return false;
        }
        self.alive_checked = Instant::now();
        !unsafe { IsWindow(Some(hwnd)) }.as_bool()
    }

    fn read(&mut self, captured: &CapturedFrame) -> Result<Size> {
        let texture = captured.texture()?;
        let content = captured.content_size()?;
        let surface = texture_size(&texture);
        let visible = Size::new(content.width.min(surface.width), content.height.min(surface.height));
        let area = self.area_within(visible)?;
        self.staging.read(&self.gpu.device, &self.gpu.context, &texture, area, &mut self.frame.bgra)?;
        self.frame.origin = self.origin_of(area);
        self.frame.width = area.width as u32;
        self.frame.height = area.height as u32;
        Ok(content)
    }

    fn area_within(&self, visible: Size) -> Result<Rect> {
        let whole = Rect::new(0, 0, visible.width as i32, visible.height as i32);
        let wanted = match self.geometry {
            Geometry::Window { crop: None, .. } => whole,
            Geometry::Window { crop: Some(crop), .. } => crop,
            Geometry::Screen(area) => area.area_in_monitor(),
        };
        wanted.intersection(whole).ok_or_else(|| Error::invalid("the recorded area is no longer visible"))
    }

    fn origin_of(&self, area: Rect) -> Point {
        match self.geometry {
            Geometry::Window { .. } => Point::new(area.x, area.y),
            Geometry::Screen(monitor) => Point::new(monitor.monitor_rect.x + area.x, monitor.monitor_rect.y + area.y),
        }
    }

    fn no_frames(&self) -> Error {
        match self.geometry {
            Geometry::Window { .. } => {
                Error::invalid("the window produced no frames within 3 s; it may be minimized or not drawing")
            }
            Geometry::Screen(_) => Error::internal("the monitor produced no frames within 3 s"),
        }
    }
}

fn geometry_of(source: RecordingSource) -> Result<(Geometry, bool)> {
    match source {
        RecordingSource::Window { window, crop } => {
            let hwnd = hwnd_from_ref(window);
            if hwnd.is_invalid() || !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
                return Err(Error::not_found(format!("window {window} does not exist")));
            }
            Ok((Geometry::Window { hwnd, crop }, false))
        }
        RecordingSource::Screen(wanted) => {
            let area = MonitorArea::locate(wanted)?;
            Ok((Geometry::Screen(area), area.is_clipped(wanted)))
        }
    }
}
