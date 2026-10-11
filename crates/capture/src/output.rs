use std::time::{Duration, Instant};

use deckhand_core::{Point, Rect, Result};
use windows::Win32::Graphics::Dxgi::{IDXGIAdapter1, IDXGIOutput};

use crate::duplication::{Duplication, DxgiFailure, Readiness};
use crate::frame::Frame;
use crate::gdi;
use crate::pixels::Block;

const RETRY_COOLDOWN: Duration = Duration::from_secs(1);

pub(crate) struct DxgiSource {
    pub adapter: IDXGIAdapter1,
    pub output: IDXGIOutput,
}

enum State {
    Closed,
    Warm(Box<Duplication>),
    CoolingDown(Instant),
    GdiOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Copied {
    Done,
    AwaitingFirstImage,
    Unavailable,
}

pub(crate) struct Output {
    monitor_id: isize,
    rect: Rect,
    sources: Vec<DxgiSource>,
    state: State,
    still_image: Option<Frame>,
    access_lost: bool,
}

impl Output {
    pub(crate) fn new(monitor_id: isize, rect: Rect, sources: Vec<DxgiSource>) -> Self {
        let state = if sources.is_empty() { State::GdiOnly } else { State::Closed };
        Self { monitor_id, rect, sources, state, still_image: None, access_lost: false }
    }

    pub(crate) const fn monitor_id(&self) -> isize {
        self.monitor_id
    }

    pub(crate) const fn rect(&self) -> Rect {
        self.rect
    }

    pub(crate) const fn access_lost(&self) -> bool {
        self.access_lost
    }

    pub(crate) fn add_source(&mut self, source: DxgiSource) {
        self.sources.push(source);
    }

    #[cfg(test)]
    pub(crate) fn mode(&self) -> &'static str {
        match &self.state {
            State::Closed => "closed",
            State::Warm(duplication) if duplication.frame_size().is_some() => "dxgi",
            State::Warm(_) => "dxgi awaiting its first image (gdi still image)",
            State::CoolingDown(_) => "gdi (dxgi cooling down)",
            State::GdiOnly => "gdi",
        }
    }

    pub(crate) fn capture_into(&mut self, part: Rect, frame: &mut Frame) -> Result<()> {
        match self.copy_with_dxgi(part, frame) {
            Copied::Done => Ok(()),
            Copied::AwaitingFirstImage => self.paste_still_image(frame),
            Copied::Unavailable => frame.paste(&gdi::capture_screen(part)?),
        }
    }

    fn paste_still_image(&mut self, frame: &mut Frame) -> Result<()> {
        let still = match self.still_image.take() {
            Some(still) => still,
            None => gdi::capture_screen(self.rect)?,
        };
        let pasted = frame.paste(&still);
        self.still_image = Some(still);
        pasted
    }

    fn copy_with_dxgi(&mut self, part: Rect, frame: &mut Frame) -> Copied {
        match self.attempt(part, frame) {
            Ok(copied) => copied,
            Err(DxgiFailure::AccessLost) => self.retry_after_access_lost(part, frame),
            Err(failure) => self.demote(failure),
        }
    }

    fn retry_after_access_lost(&mut self, part: Rect, frame: &mut Frame) -> Copied {
        self.access_lost = true;
        self.close();
        self.attempt(part, frame).unwrap_or_else(|failure| self.demote(failure))
    }

    fn demote(&mut self, failure: DxgiFailure) -> Copied {
        self.close();
        self.state = match failure {
            DxgiFailure::Unsupported => State::GdiOnly,
            DxgiFailure::Transient | DxgiFailure::AccessLost => State::CoolingDown(Instant::now() + RETRY_COOLDOWN),
        };
        Copied::Unavailable
    }

    fn close(&mut self) {
        self.state = State::Closed;
        self.still_image = None;
    }

    fn attempt(&mut self, part: Rect, frame: &mut Frame) -> Result<Copied, DxgiFailure> {
        let rect = self.rect;
        let Some(duplication) = self.ready_duplication()? else {
            return Ok(Copied::Unavailable);
        };
        if duplication.update()? == Readiness::AwaitingFirstImage {
            return Ok(Copied::AwaitingFirstImage);
        }
        if duplication.frame_size() != Some((rect.width as u32, rect.height as u32)) {
            return Err(DxgiFailure::AccessLost);
        }
        let from = Block {
            x: (part.x - rect.x) as usize,
            y: (part.y - rect.y) as usize,
            width: part.width as usize,
            height: part.height as usize,
        };
        let to = frame.offset_of(Point::new(part.x, part.y));
        duplication.read(from, &mut frame.pixels_mut(), to)?;
        self.still_image = None;
        Ok(Copied::Done)
    }

    fn ready_duplication(&mut self) -> Result<Option<&mut Duplication>, DxgiFailure> {
        if let State::CoolingDown(until) = self.state {
            if Instant::now() < until {
                return Ok(None);
            }
            self.close();
        }
        if matches!(self.state, State::Closed) {
            self.still_image = None;
            self.state = State::Warm(Box::new(open_first(&self.sources)?));
        }
        match &mut self.state {
            State::Warm(duplication) => Ok(Some(duplication)),
            _ => Ok(None),
        }
    }
}

fn open_first(sources: &[DxgiSource]) -> Result<Duplication, DxgiFailure> {
    let mut failure = DxgiFailure::Unsupported;
    for source in sources {
        match Duplication::open(&source.adapter, &source.output) {
            Ok(duplication) => return Ok(duplication),
            Err(next) => failure = more_hopeful(failure, next),
        }
    }
    Err(failure)
}

fn more_hopeful(current: DxgiFailure, next: DxgiFailure) -> DxgiFailure {
    match (current, next) {
        (DxgiFailure::Transient | DxgiFailure::AccessLost, DxgiFailure::Unsupported) => current,
        _ => next,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_transient_failure_on_any_adapter_keeps_dxgi_alive() {
        use DxgiFailure::{AccessLost, Transient, Unsupported};
        assert_eq!(more_hopeful(Transient, Unsupported), Transient);
        assert_eq!(more_hopeful(Unsupported, Transient), Transient);
        assert_eq!(more_hopeful(Unsupported, Unsupported), Unsupported);
        assert_eq!(more_hopeful(AccessLost, Unsupported), AccessLost);
    }

    #[test]
    fn outputs_without_adapters_use_gdi() {
        let output = Output::new(1, Rect::new(0, 0, 10, 10), Vec::new());
        assert_eq!(output.mode(), "gdi");
    }

    #[test]
    fn gdi_outputs_fill_their_part_of_the_frame() {
        crate::dpi::make_thread_per_monitor_aware().unwrap();
        let mut output = Output::new(1, Rect::new(0, 0, 10, 10), Vec::new());
        let mut frame = Frame::black(Point::new(-4, -4), 8, 8);
        output.capture_into(Rect::new(0, 0, 4, 4), &mut frame).unwrap();
        assert_eq!(frame.bgra.len(), 8 * 8 * 4);
        assert_eq!(output.mode(), "gdi");
    }
}
