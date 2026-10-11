use std::sync::mpsc::{Receiver, Sender};

use deckhand_core::{Error, Rect, Result, WindowRef};

use crate::com::ComApartment;
use crate::compose::capture_rect;
use crate::displays::Displays;
use crate::dpi::make_thread_per_monitor_aware;
use crate::frame::{Frame, validate_capture_rect};
use crate::window::capture_window;

pub(crate) type Reply<T> = Sender<Result<T>>;

pub(crate) enum Request {
    MonitorRects(Reply<Vec<Rect>>),
    Screen(Rect, Reply<Frame>),
    Monitor(usize, Reply<Frame>),
    Window(WindowRef, Reply<Frame>),
    #[cfg(test)]
    Modes(Reply<Vec<&'static str>>),
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
    let mut displays = Displays::default();
    while let Ok(request) = inbox.recv() {
        handle(&mut displays, request);
    }
}

fn handle(displays: &mut Displays, request: Request) {
    match request {
        Request::MonitorRects(reply) => answer(&reply, monitor_rects(displays)),
        Request::Screen(rect, reply) => answer(&reply, screen(displays, rect)),
        Request::Monitor(index, reply) => answer(&reply, monitor(displays, index)),
        Request::Window(window, reply) => answer(&reply, capture_window(window, |rect| screen(displays, rect))),
        #[cfg(test)]
        Request::Modes(reply) => answer(&reply, Ok(displays.modes())),
    }
}

fn answer<T>(reply: &Reply<T>, result: Result<T>) {
    let _ = reply.send(result);
}

fn monitor_rects(displays: &mut Displays) -> Result<Vec<Rect>> {
    displays.refresh()?;
    Ok(displays.rects())
}

fn screen(displays: &mut Displays, rect: Rect) -> Result<Frame> {
    validate_capture_rect(rect)?;
    displays.refresh()?;
    capture_rect(displays.outputs_mut(), rect)
}

fn monitor(displays: &mut Displays, index: usize) -> Result<Frame> {
    displays.refresh()?;
    let rects = displays.rects();
    let rect = rects.get(index).ok_or_else(|| {
        Error::invalid(format!("monitor index {index} is out of range; {} monitor(s) are attached", rects.len()))
    })?;
    capture_rect(displays.outputs_mut(), *rect)
}
