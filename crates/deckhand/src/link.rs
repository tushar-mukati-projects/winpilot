use std::{
    collections::HashMap,
    io::{BufReader, BufWriter, Read, Write},
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, AtomicU32, Ordering},
        mpsc,
    },
    time::Duration,
};

use deckhand_core::{
    Context, Error, ErrorCode, Result,
    ipc::{FromWorker, ToWorker, read_frame, write_frame},
    protocol::Reply,
};
use parking_lot::Mutex;
use tokio::sync::oneshot;

pub type ReplyReceiver = oneshot::Receiver<Result<Reply>>;
type ReplySender = oneshot::Sender<Result<Reply>>;

pub trait WorkerProcess: Send {
    fn terminate(&mut self);
}

pub struct WorkerPipes {
    pub stdin: Box<dyn Write + Send>,
    pub stdout: Box<dyn Read + Send>,
    pub process: Box<dyn WorkerProcess>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Death {
    Timeout { tool: &'static str, deadline: Duration },
    Exited,
    Unreadable(String),
    Unwritable(String),
    Shutdown,
}

impl Death {
    pub fn error(&self) -> Error {
        match self {
            Self::Shutdown => Error::new(ErrorCode::Cancelled, "the deckhand server is shutting down"),
            _ => Error::new(ErrorCode::TargetNotResponding, self.restart_message()),
        }
    }

    fn restart_message(&self) -> String {
        let cause = match self {
            Self::Timeout { tool, deadline } => {
                format!("the desktop worker did not answer {tool} within {} (an app may be hung)", seconds(*deadline))
            }
            Self::Exited => "the desktop worker exited unexpectedly".to_string(),
            Self::Unreadable(detail) => format!("the desktop worker sent an unreadable message ({detail})"),
            Self::Unwritable(detail) => format!("the desktop worker stopped accepting calls ({detail})"),
            Self::Shutdown => "the deckhand server is shutting down".to_string(),
        };
        format!("{cause}; it was restarted")
    }

    const fn counts_as_restart(&self) -> bool {
        !matches!(self, Self::Shutdown)
    }
}

pub fn seconds(duration: Duration) -> String {
    if duration.subsec_millis() == 0 {
        format!("{} s", duration.as_secs())
    } else {
        format!("{:.1} s", duration.as_secs_f64())
    }
}

pub fn stopped_error() -> Error {
    Error::new(ErrorCode::TargetNotResponding, "the desktop worker stopped before answering; try again")
}

pub struct Link {
    frames: Mutex<Option<mpsc::Sender<ToWorker>>>,
    pending: Mutex<Option<HashMap<u64, ReplySender>>>,
    process: Mutex<Option<Box<dyn WorkerProcess>>>,
    started: AtomicBool,
    restarts: Arc<AtomicU32>,
}

impl Link {
    pub fn start(pipes: WorkerPipes, restarts: Arc<AtomicU32>) -> Result<(Arc<Self>, oneshot::Receiver<String>)> {
        let (frames, outbox) = mpsc::channel();
        let link = Arc::new(Self {
            frames: Mutex::new(Some(frames)),
            pending: Mutex::new(Some(HashMap::new())),
            process: Mutex::new(Some(pipes.process)),
            started: AtomicBool::new(false),
            restarts,
        });
        let (ready, announced) = oneshot::channel();
        let writer = Arc::downgrade(&link);
        let reader = Arc::downgrade(&link);
        spawn_named("deckhand-worker-writer", move || write_frames(&writer, pipes.stdin, &outbox))?;
        spawn_named("deckhand-worker-reader", move || read_frames(&reader, pipes.stdout, ready))?;
        Ok((link, announced))
    }

    pub fn is_alive(&self) -> bool {
        self.pending.lock().is_some()
    }

    pub fn mark_started(&self) {
        self.started.store(true, Ordering::SeqCst);
    }

    pub fn register(&self, id: u64) -> Result<ReplyReceiver> {
        let mut pending = self.pending.lock();
        let Some(calls) = pending.as_mut() else { return Err(stopped_error()) };
        let (sender, receiver) = oneshot::channel();
        calls.insert(id, sender);
        Ok(receiver)
    }

    pub fn forget(&self, id: u64) {
        if let Some(calls) = self.pending.lock().as_mut() {
            calls.remove(&id);
        }
    }

    pub fn send(&self, frame: ToWorker) -> Result<()> {
        let frames = self.frames.lock();
        let Some(sender) = frames.as_ref() else { return Err(stopped_error()) };
        sender.send(frame).map_err(|_| stopped_error())
    }

    pub fn stop(&self, death: &Death) {
        let Some(calls) = self.pending.lock().take() else { return };
        drop(self.frames.lock().take());
        let process = self.process.lock().take();
        if let Some(mut process) = process {
            process.terminate();
        }
        if death.counts_as_restart() && self.started.load(Ordering::SeqCst) {
            self.restarts.fetch_add(1, Ordering::SeqCst);
        }
        let error = death.error();
        for sender in calls.into_values() {
            let _ = sender.send(Err(error.clone()));
        }
    }

    fn deliver(&self, id: u64, result: Result<Reply>) {
        let sender = self.pending.lock().as_mut().and_then(|calls| calls.remove(&id));
        if let Some(sender) = sender {
            let _ = sender.send(result);
        }
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.stop(&Death::Shutdown);
    }
}

fn spawn_named(name: &str, body: impl FnOnce() + Send + 'static) -> Result<()> {
    std::thread::Builder::new().name(name.to_string()).spawn(body).map(drop).context("starting a worker pipe thread")
}

fn write_frames(link: &Weak<Link>, stdin: Box<dyn Write + Send>, outbox: &mpsc::Receiver<ToWorker>) {
    let mut stdin = BufWriter::new(stdin);
    while let Ok(frame) = outbox.recv() {
        if let Err(error) = write_frame(&mut stdin, &frame) {
            stop_link(link, &Death::Unwritable(error.to_string()));
            return;
        }
    }
}

fn read_frames(link: &Weak<Link>, stdout: Box<dyn Read + Send>, ready: oneshot::Sender<String>) {
    let mut stdout = BufReader::new(stdout);
    let mut ready = Some(ready);
    let death = loop {
        match read_frame::<_, FromWorker>(&mut stdout) {
            Ok(Some(FromWorker::Ready { version })) => announce(&mut ready, version),
            Ok(Some(FromWorker::Reply { id, result })) => {
                let Some(link) = link.upgrade() else { return };
                link.deliver(id, *result);
            }
            Ok(None) => break Death::Exited,
            Err(error) => break Death::Unreadable(error.to_string()),
        }
    };
    stop_link(link, &death);
}

fn announce(ready: &mut Option<oneshot::Sender<String>>, version: String) {
    if let Some(sender) = ready.take() {
        let _ = sender.send(version);
    }
}

fn stop_link(link: &Weak<Link>, death: &Death) {
    if let Some(link) = link.upgrade() {
        link.stop(death);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timeout_message_names_the_tool_and_deadline() {
        let death = Death::Timeout { tool: "observe", deadline: Duration::from_secs(10) };
        let error = death.error();
        assert_eq!(error.code, ErrorCode::TargetNotResponding);
        assert_eq!(
            error.message,
            "the desktop worker did not answer observe within 10 s (an app may be hung); it was restarted"
        );
    }

    #[test]
    fn fractional_deadlines_show_one_decimal() {
        assert_eq!(seconds(Duration::from_millis(5500)), "5.5 s");
        assert_eq!(seconds(Duration::from_secs(3)), "3 s");
    }

    #[test]
    fn shutdown_is_not_a_restart() {
        assert!(!Death::Shutdown.counts_as_restart());
        assert!(Death::Exited.counts_as_restart());
        assert_eq!(Death::Shutdown.error().code, ErrorCode::Cancelled);
    }
}
