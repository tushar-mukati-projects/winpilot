use std::sync::Arc;
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};

use winpilot_core::{Context, Error, Rect, Result, WindowRef};

use crate::actor::{self, Reply, Request};
use crate::frame::Frame;

#[derive(Clone)]
pub struct Capturer {
    shared: Arc<Shared>,
}

struct Shared {
    requests: Option<Sender<Request>>,
    worker: Option<JoinHandle<()>>,
}

impl Drop for Shared {
    fn drop(&mut self) {
        drop(self.requests.take());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Capturer {
    pub fn spawn() -> Result<Self> {
        let (requests, inbox) = mpsc::channel();
        let (ready, started) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("winpilot-capture".to_string())
            .spawn(move || actor::run(&inbox, &ready))
            .context("starting the capture thread")?;
        let startup = started.recv().unwrap_or_else(|_| Err(Error::internal("capture thread stopped during startup")));
        if let Err(error) = startup {
            let _ = worker.join();
            return Err(error);
        }
        Ok(Self { shared: Arc::new(Shared { requests: Some(requests), worker: Some(worker) }) })
    }

    pub fn monitor_rects(&self) -> Result<Vec<Rect>> {
        self.call(Request::MonitorRects)
    }

    pub fn screen(&self, rect: Rect) -> Result<Frame> {
        self.call(|reply| Request::Screen(rect, reply))
    }

    pub fn monitor(&self, index: usize) -> Result<Frame> {
        self.call(|reply| Request::Monitor(index, reply))
    }

    pub fn window(&self, hwnd: WindowRef) -> Result<Frame> {
        self.call(|reply| Request::Window(hwnd, reply))
    }

    #[cfg(test)]
    fn output_modes(&self) -> Result<Vec<&'static str>> {
        self.call(Request::Modes)
    }

    fn call<T>(&self, request: impl FnOnce(Reply<T>) -> Request) -> Result<T> {
        let stopped = || Error::internal("capture thread has stopped");
        let (reply, response) = mpsc::channel();
        let requests = self.shared.requests.as_ref().ok_or_else(stopped)?;
        requests.send(request(reply)).map_err(|_| stopped())?;
        response.recv().map_err(|_| stopped())?
    }
}

#[cfg(test)]
mod live_tests;
