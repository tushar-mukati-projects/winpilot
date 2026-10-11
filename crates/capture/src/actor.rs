use std::sync::mpsc::{Receiver, Sender};

use deckhand_core::{Error, Rect, Result, WindowRef};

use crate::capture_area::CaptureArea;
use crate::com::ComApartment;
use crate::compose::capture_area;
use crate::displays::Displays;
use crate::dpi::make_thread_per_monitor_aware;
use crate::frame::Frame;
use crate::screen_grabber::ScreenGrabber;
use crate::window::capture_window;

pub(crate) type Reply<T> = Sender<Result<T>>;

pub(crate) enum Request {
    MonitorRects(Reply<Vec<Rect>>),
    Screen(Rect, Reply<Frame>),
    Monitor(usize, Reply<Frame>),
    Window(WindowRef, Reply<Frame>),
    #[cfg(test)]
    Modes(Reply<Vec<&'static str>>),
    #[cfg(test)]
    Served(Reply<Vec<crate::output::Served>>),
}

#[derive(Default)]
struct Screen {
    displays: Displays,
    grabber: ScreenGrabber,
}

pub(crate) fn run(inbox: &Receiver<Request>, ready: &Sender<Result<()>>) {
    let apartment = match prepare_thread() {
        Ok(apartment) => apartment,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    if ready.send(Ok(())).is_ok() {
        serve(inbox);
    }
    drop(apartment);
}

fn prepare_thread() -> Result<ComApartment> {
    let apartment = ComApartment::enter_mta()?;
    make_thread_per_monitor_aware()?;
    Ok(apartment)
}

fn serve(inbox: &Receiver<Request>) {
    let mut screen = Screen::default();
    while let Ok(request) = inbox.recv() {
        handle(&mut screen, request);
    }
}

fn handle(screen: &mut Screen, request: Request) {
    match request {
        Request::MonitorRects(reply) => answer(&reply, screen.monitor_rects()),
        Request::Screen(rect, reply) => answer(&reply, CaptureArea::new(rect).and_then(|area| screen.capture(area))),
        Request::Monitor(index, reply) => answer(&reply, screen.monitor(index)),
        Request::Window(window, reply) => answer(&reply, capture_window(window, |area| screen.capture(area))),
        #[cfg(test)]
        Request::Modes(reply) => answer(&reply, Ok(screen.displays.modes())),
        #[cfg(test)]
        Request::Served(reply) => answer(&reply, Ok(screen.displays.served())),
    }
}

fn answer<T>(reply: &Reply<T>, result: Result<T>) {
    let _ = reply.send(result);
}

impl Screen {
    fn monitor_rects(&mut self) -> Result<Vec<Rect>> {
        self.displays.refresh()?;
        Ok(self.displays.rects())
    }

    fn capture(&mut self, area: CaptureArea) -> Result<Frame> {
        self.displays.refresh()?;
        capture_area(self.displays.outputs_mut(), &mut self.grabber, area)
    }

    fn monitor(&mut self, index: usize) -> Result<Frame> {
        let rects = self.monitor_rects()?;
        let rect = rects.get(index).ok_or_else(|| {
            Error::invalid(format!("monitor index {index} is out of range; {} monitor(s) are attached", rects.len()))
        })?;
        let area = CaptureArea::new(*rect)?;
        capture_area(self.displays.outputs_mut(), &mut self.grabber, area)
    }
}
