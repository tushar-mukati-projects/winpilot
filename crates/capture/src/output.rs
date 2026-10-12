use std::time::{Duration, Instant};

use deckhand_core::{Point, Rect, Result};
use windows::Win32::Graphics::Dxgi::{IDXGIAdapter1, IDXGIOutput};

use crate::duplication::{Duplication, DxgiFailure};
use crate::frame::Frame;
use crate::freshness::{Decision, decide};
use crate::pixels::Block;
use crate::screen_grabber::ScreenGrabber;

const RETRY_COOLDOWN: Duration = Duration::from_secs(1);

pub(crate) struct DxgiSource {
    pub adapter: IDXGIAdapter1,
    pub output: IDXGIOutput,
}

enum State {
    Closed,
    Open(Box<Duplication>),
    CoolingDown(Instant),
    GdiOnly,
}

pub(crate) struct Output {
    monitor_id: isize,
    rect: Rect,
    sources: Vec<DxgiSource>,
    state: State,
    needs_rebuild: bool,
}

impl Output {
    pub(crate) fn new(monitor_id: isize, rect: Rect, sources: Vec<DxgiSource>) -> Self {
        let state = if sources.is_empty() { State::GdiOnly } else { State::Closed };
        Self { monitor_id, rect, sources, state, needs_rebuild: false }
    }

    pub(crate) const fn monitor_id(&self) -> isize {
        self.monitor_id
    }

    pub(crate) const fn rect(&self) -> Rect {
        self.rect
    }

    pub(crate) const fn needs_rebuild(&self) -> bool {
        self.needs_rebuild
    }

    pub(crate) fn add_source(&mut self, source: DxgiSource) {
        self.sources.push(source);
    }

    #[cfg(test)]
    pub(crate) fn mode(&self) -> &'static str {
        match &self.state {
            State::Closed => "closed",
            State::Open(duplication) => {
                match decide(duplication.opened_at(), duplication.last_frame_at(), Instant::now()) {
                    Decision::UseStaging => "dxgi",
                    Decision::UseGdi | Decision::Demote => "gdi (dxgi open without a fresh frame)",
                }
            }
            State::CoolingDown(_) => "gdi (dxgi cooling down)",
            State::GdiOnly => "gdi",
        }
    }

    pub(crate) fn capture_into(&mut self, part: Rect, frame: &mut Frame, grabber: &mut ScreenGrabber) -> Result<()> {
        if self.copy_fresh_staging(part, frame) {
            return Ok(());
        }
        grabber.grab_into(part, frame)
    }

    fn copy_fresh_staging(&mut self, part: Rect, frame: &mut Frame) -> bool {
        if self.choose_source(Instant::now()) != Decision::UseStaging {
            return false;
        }
        match self.read_staging(part, frame) {
            Ok(()) => true,
            Err(failure) => {
                self.after_failed_read(failure);
                false
            }
        }
    }

    fn choose_source(&mut self, now: Instant) -> Decision {
        let decision = match self.poll(now) {
            Ok(decision) => decision,
            Err(DxgiFailure::AccessLost) => self.reopen_after_access_lost(now),
            Err(failure) => self.back_off(failure, now),
        };
        if decision == Decision::Demote {
            self.state = State::GdiOnly;
        }
        decision
    }

    fn reopen_after_access_lost(&mut self, now: Instant) -> Decision {
        self.state = State::Closed;
        match self.poll(now) {
            Ok(decision) => {
                self.needs_rebuild = false;
                decision
            }
            Err(failure) => {
                self.needs_rebuild = true;
                self.back_off(failure, now)
            }
        }
    }

    fn after_failed_read(&mut self, failure: DxgiFailure) {
        if failure == DxgiFailure::AccessLost {
            self.needs_rebuild = true;
            self.state = State::Closed;
            return;
        }
        self.back_off(failure, Instant::now());
    }

    fn back_off(&mut self, failure: DxgiFailure, now: Instant) -> Decision {
        self.state = match failure {
            DxgiFailure::Unsupported => State::GdiOnly,
            DxgiFailure::Transient | DxgiFailure::AccessLost => State::CoolingDown(now + RETRY_COOLDOWN),
        };
        Decision::UseGdi
    }

    fn poll(&mut self, now: Instant) -> Result<Decision, DxgiFailure> {
        self.leave_expired_cooldown(now);
        if matches!(self.state, State::Closed) {
            self.state = State::Open(Box::new(open_first(&self.sources, now)?));
            return Ok(Decision::UseGdi);
        }
        let State::Open(duplication) = &mut self.state else {
            return Ok(Decision::UseGdi);
        };
        duplication.poll(now)?;
        Ok(decide(duplication.opened_at(), duplication.last_frame_at(), now))
    }

    fn leave_expired_cooldown(&mut self, now: Instant) {
        if let State::CoolingDown(until) = self.state
            && now >= until
        {
            self.state = State::Closed;
        }
    }

    fn read_staging(&self, part: Rect, frame: &mut Frame) -> Result<(), DxgiFailure> {
        let State::Open(duplication) = &self.state else {
            return Err(DxgiFailure::Transient);
        };
        if duplication.frame_size() != Some((self.rect.width as u32, self.rect.height as u32)) {
            return Err(DxgiFailure::AccessLost);
        }
        let from = Block {
            x: (part.x - self.rect.x) as usize,
            y: (part.y - self.rect.y) as usize,
            width: part.width as usize,
            height: part.height as usize,
        };
        let to = frame.offset_of(Point::new(part.x, part.y));
        duplication.read(from, &mut frame.pixels_mut(), to)
    }
}

fn open_first(sources: &[DxgiSource], now: Instant) -> Result<Duplication, DxgiFailure> {
    let mut failure = DxgiFailure::Unsupported;
    for source in sources {
        match Duplication::open(&source.adapter, &source.output, now) {
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
    fn gdi_outputs_fill_their_part_of_the_frame_live() {
        crate::dpi::make_thread_per_monitor_aware().unwrap();
        let mut output = Output::new(1, Rect::new(0, 0, 10, 10), Vec::new());
        let mut frame = Frame::black(Point::new(-4, -4), 8, 8);
        output.capture_into(Rect::new(0, 0, 4, 4), &mut frame, &mut ScreenGrabber::default()).unwrap();
        assert_eq!(frame.bgra.len(), 8 * 8 * 4);
        assert_eq!(output.mode(), "gdi");
    }

    #[test]
    fn unsupported_failures_pin_gdi_and_others_cool_down() {
        let mut output = Output::new(1, Rect::new(0, 0, 10, 10), Vec::new());
        let now = Instant::now();
        assert_eq!(output.back_off(DxgiFailure::Transient, now), Decision::UseGdi);
        assert_eq!(output.mode(), "gdi (dxgi cooling down)");
        output.back_off(DxgiFailure::Unsupported, now);
        assert_eq!(output.mode(), "gdi");
    }

    #[test]
    fn the_rebuild_flag_follows_the_outcome_of_the_reopen_after_access_lost() {
        crate::dpi::make_thread_per_monitor_aware().unwrap();
        let mut displays = crate::displays::Displays::default();
        displays.refresh().unwrap();
        let Some(output) = displays.outputs_mut().iter_mut().find(|output| !output.sources.is_empty()) else {
            return;
        };
        output.needs_rebuild = true;
        assert_eq!(output.reopen_after_access_lost(Instant::now()), Decision::UseGdi);
        let reopened = matches!(output.state, State::Open(_));
        assert_eq!(output.needs_rebuild(), !reopened);
    }

    #[test]
    fn a_lost_staging_read_asks_for_a_rebuild() {
        let mut output = Output::new(1, Rect::new(0, 0, 10, 10), Vec::new());
        output.after_failed_read(DxgiFailure::AccessLost);
        assert!(output.needs_rebuild());
        assert_eq!(output.mode(), "closed");
    }
}
