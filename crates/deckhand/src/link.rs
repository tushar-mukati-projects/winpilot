use std::{
    collections::HashMap,
    io::{BufReader, BufWriter, Read, Write},
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
};

use deckhand_core::{
    Context, Error, Result,
    ipc::{FromWorker, ToWorker, read_frame, write_frame},
    protocol::Reply,
};
use parking_lot::Mutex;
use tokio::sync::oneshot;

use crate::death::{Death, stopped_error};

pub type ReplyReceiver = oneshot::Receiver<Result<Reply>>;
type ReplySender = oneshot::Sender<Result<Reply>>;
type Pending = HashMap<u64, ReplySender>;
pub type DeathHook = Box<dyn Fn(&Link, &Death, bool) + Send + Sync>;

pub trait WorkerProcess: Send {
    fn terminate(&mut self);
}

pub struct WorkerPipes {
    pub stdin: Box<dyn Write + Send>,
    pub stdout: Box<dyn Read + Send>,
    pub process: Box<dyn WorkerProcess>,
}

pub struct Link {
    frames: Mutex<Option<mpsc::Sender<ToWorker>>>,
    pending: Mutex<Option<Pending>>,
    process: Mutex<Option<Box<dyn WorkerProcess>>>,
    started: AtomicBool,
    answered: AtomicBool,
    on_death: DeathHook,
}

impl Link {
    pub fn start(pipes: WorkerPipes, on_death: DeathHook) -> Result<(Arc<Self>, oneshot::Receiver<String>)> {
        let (frames, outbox) = mpsc::channel();
        let link = Arc::new(Self {
            frames: Mutex::new(Some(frames)),
            pending: Mutex::new(Some(HashMap::new())),
            process: Mutex::new(Some(pipes.process)),
            started: AtomicBool::new(false),
            answered: AtomicBool::new(false),
            on_death,
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

    pub fn is_started(&self) -> bool {
        self.started.load(Ordering::SeqCst)
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
        let Some(calls) = self.mark_dead_and_kill() else { return };
        let in_use = !calls.is_empty() || self.answered.load(Ordering::SeqCst);
        (self.on_death)(self, death, in_use);
        fail_pending(calls, &death.error());
    }

    fn mark_dead_and_kill(&self) -> Option<Pending> {
        let calls = self.pending.lock().take()?;
        drop(self.frames.lock().take());
        let process = self.process.lock().take();
        if let Some(mut process) = process {
            process.terminate();
        }
        Some(calls)
    }

    fn deliver(&self, id: u64, result: Result<Reply>) {
        self.answered.store(true, Ordering::SeqCst);
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

fn fail_pending(calls: Pending, error: &Error) {
    for sender in calls.into_values() {
        let _ = sender.send(Err(error.clone()));
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
