use std::{
    collections::HashMap,
    io::{BufReader, BufWriter, Read, Write},
    sync::{
        Arc, Weak,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

use parking_lot::Mutex;
use tokio::sync::oneshot;
use winpilot_core::{
    Context, Error, Result,
    ipc::{FromWorker, ToWorker, read_frame, write_frame},
    protocol::Reply,
};

use crate::death::{Death, stopped_error};

type ReplyReceiver = oneshot::Receiver<Result<Reply>>;
type ReplySender = oneshot::Sender<Result<Reply>>;
type Pending = HashMap<u64, ReplySender>;
pub type DeathHook = Box<dyn Fn(&Link, &Death, bool) + Send + Sync>;

const GRACEFUL_EXIT: Duration = Duration::from_secs(3);

pub trait WorkerProcess: Send {
    fn exits_within(&mut self, patience: Duration) -> bool;
    fn terminate(&mut self);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Announced {
    pub version: String,
    pub build: String,
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
    pub fn start(pipes: WorkerPipes, on_death: DeathHook) -> Result<(Arc<Self>, oneshot::Receiver<Announced>)> {
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
        spawn_named("winpilot-worker-writer", move || write_frames(&writer, pipes.stdin, &outbox))?;
        spawn_named("winpilot-worker-reader", move || read_frames(&reader, pipes.stdout, ready))?;
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
        let Some(calls) = self.mark_dead() else { return };
        self.end_process(death);
        let in_use = !calls.is_empty() || self.answered.load(Ordering::SeqCst);
        (self.on_death)(self, death, in_use);
        fail_pending(calls, &death.error());
    }

    fn mark_dead(&self) -> Option<Pending> {
        let calls = self.pending.lock().take()?;
        drop(self.frames.lock().take());
        Some(calls)
    }

    fn end_process(&self, death: &Death) {
        let Some(mut process) = self.process.lock().take() else { return };
        if death.lets_the_worker_finish() && process.exits_within(GRACEFUL_EXIT) {
            return;
        }
        process.terminate();
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

fn read_frames(link: &Weak<Link>, stdout: Box<dyn Read + Send>, ready: oneshot::Sender<Announced>) {
    let mut stdout = BufReader::new(stdout);
    let mut ready = Some(ready);
    let death = loop {
        match read_frame::<_, FromWorker>(&mut stdout) {
            Ok(Some(FromWorker::Ready { version, build })) => announce(&mut ready, Announced { version, build }),
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

fn announce(ready: &mut Option<oneshot::Sender<Announced>>, announced: Announced) {
    if let Some(sender) = ready.take() {
        let _ = sender.send(announced);
    }
}

fn stop_link(link: &Weak<Link>, death: &Death) {
    if let Some(link) = link.upgrade() {
        link.stop(death);
    }
}

#[cfg(test)]
mod tests {
    use std::io::{PipeWriter, Read};

    use super::*;

    type Events = Arc<Mutex<Vec<&'static str>>>;

    struct Watched {
        events: Events,
        exits: bool,
    }

    impl WorkerProcess for Watched {
        fn exits_within(&mut self, _patience: Duration) -> bool {
            self.events.lock().push("wait");
            self.exits
        }

        fn terminate(&mut self) {
            self.events.lock().push("terminate");
        }
    }

    struct Wired {
        link: Arc<Link>,
        worker_input: std::io::PipeReader,
        _worker_output: PipeWriter,
        events: Events,
    }

    fn wired(exits: bool) -> Wired {
        let (worker_input, stdin) = std::io::pipe().unwrap();
        let (stdout, worker_output) = std::io::pipe().unwrap();
        let events = Events::default();
        let process = Box::new(Watched { events: Arc::clone(&events), exits });
        let pipes = WorkerPipes { stdin: Box::new(stdin), stdout: Box::new(stdout), process };
        let (link, _announced) = Link::start(pipes, Box::new(|_, _, _| {})).unwrap();
        Wired { link, worker_input, _worker_output: worker_output, events }
    }

    #[test]
    fn shutdown_closes_the_worker_input_and_waits_before_any_kill() {
        let mut wired = wired(true);
        wired.link.stop(&Death::Shutdown);
        let mut rest = Vec::new();
        wired.worker_input.read_to_end(&mut rest).unwrap();
        assert!(rest.is_empty());
        assert_eq!(*wired.events.lock(), ["wait"]);
        assert!(!wired.link.is_alive());
    }

    #[test]
    fn a_worker_that_does_not_exit_in_time_is_terminated() {
        let wired = wired(false);
        wired.link.stop(&Death::Shutdown);
        assert_eq!(*wired.events.lock(), ["wait", "terminate"]);
    }

    #[test]
    fn a_deadline_kill_terminates_at_once() {
        let wired = wired(true);
        wired.link.stop(&Death::Timeout { tool: "act", deadline: Duration::from_secs(1) });
        assert_eq!(*wired.events.lock(), ["terminate"]);
        wired.link.stop(&Death::Shutdown);
        assert_eq!(*wired.events.lock(), ["terminate"]);
    }
}
