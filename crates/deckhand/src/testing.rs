use std::{
    fs,
    io::{PipeReader, PipeWriter, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use deckhand_core::{
    Context, Result,
    ipc::{FromWorker, ToWorker, read_frame, write_frame},
    protocol::{ActArgs, AfterMode, Call, ClipboardReport, Reply, StatusReport, Step},
};

use crate::{
    lifecycle::Spawn,
    link::{WorkerPipes, WorkerProcess},
};

pub struct FakeWorker {
    input: PipeReader,
    output: PipeWriter,
}

impl FakeWorker {
    pub fn receive(&mut self) -> Option<ToWorker> {
        read_frame(&mut self.input).ok().flatten()
    }

    pub fn handshake(&mut self) -> bool {
        self.handshake_as(crate::BUILD_ID)
    }

    pub fn handshake_as(&mut self, build: &str) -> bool {
        let ready = FromWorker::Ready { version: crate::VERSION.to_string(), build: build.to_string() };
        matches!(self.receive(), Some(ToWorker::Init { .. })) && write_frame(&mut self.output, &ready).is_ok()
    }

    pub fn reply(&mut self, id: u64, result: Result<Reply>) -> bool {
        write_frame(&mut self.output, &FromWorker::Reply { id, result: Box::new(result) }).is_ok()
    }

    pub fn answer(&mut self, message: &ToWorker) -> bool {
        match message {
            ToWorker::Call { id, call, .. } => self.reply(*id, Ok(echo(call))),
            _ => true,
        }
    }

    pub fn write_raw(&mut self, bytes: &[u8]) {
        let _ = self.output.write_all(bytes);
    }
}

type Behaviour = dyn Fn(usize, FakeWorker) + Send + Sync;

#[derive(Clone)]
pub struct FakeSpawner {
    behaviour: Arc<Behaviour>,
    spawned: Arc<AtomicUsize>,
}

impl FakeSpawner {
    pub fn new(behaviour: impl Fn(usize, FakeWorker) + Send + Sync + 'static) -> Self {
        Self { behaviour: Arc::new(behaviour), spawned: Arc::new(AtomicUsize::new(0)) }
    }

    pub fn spawned(&self) -> usize {
        self.spawned.load(Ordering::SeqCst)
    }
}

impl Spawn for FakeSpawner {
    fn spawn(&self) -> Result<WorkerPipes> {
        let (stdout, worker_output) = std::io::pipe().context("creating a fake worker pipe")?;
        let (worker_input, stdin) = std::io::pipe().context("creating a fake worker pipe")?;
        let index = self.spawned.fetch_add(1, Ordering::SeqCst);
        let behaviour = Arc::clone(&self.behaviour);
        let worker = FakeWorker { input: worker_input, output: worker_output };
        std::thread::spawn(move || behaviour(index, worker));
        Ok(WorkerPipes { stdin: Box::new(stdin), stdout: Box::new(stdout), process: Box::new(FakeProcess) })
    }
}

struct FakeProcess;

impl WorkerProcess for FakeProcess {
    fn exits_within(&mut self, _patience: std::time::Duration) -> bool {
        true
    }

    fn terminate(&mut self) {}
}

pub fn echo_worker(mut worker: FakeWorker) {
    if worker.handshake() {
        echo_after_handshake(worker);
    }
}

pub fn hang_worker(mut worker: FakeWorker) {
    if worker.handshake() {
        drain(worker);
    }
}

pub fn drain(mut worker: FakeWorker) {
    while worker.receive().is_some() {}
}

pub fn echo_after_handshake(mut worker: FakeWorker) {
    while let Some(message) = worker.receive() {
        worker.answer(&message);
    }
}

pub fn echo(call: &Call) -> Reply {
    if call.is_status() {
        return Reply::Status(status_report());
    }
    let text = clipboard_text(call).unwrap_or_else(|| call.label().to_string());
    Reply::Clipboard(ClipboardReport { text: Some(text), truncated: false })
}

pub fn clipboard_text(call: &Call) -> Option<String> {
    let Call::Act(args) = call else { return None };
    args.steps.iter().find_map(|step| match step {
        Step::Clipboard { text } => Some(text.clone()),
        _ => None,
    })
}

pub fn set_clipboard(text: &str) -> Call {
    let steps = vec![Step::Clipboard { text: text.to_string() }];
    Call::Act(ActArgs { steps, after: AfterMode::None, stop_on_error: true, background: None })
}

pub fn status_report() -> StatusReport {
    StatusReport {
        version: "test".to_string(),
        uptime_ms: 5,
        stopped: false,
        user_idle_ms: Some(1000),
        kill_hotkey: "ctrl+alt+escape".to_string(),
        hotkey_registered: true,
        elevated: false,
        secure_desktop: false,
        worker_restarts: 0,
        recent: Vec::new(),
    }
}

pub struct TempDir {
    path: PathBuf,
}

impl TempDir {
    pub fn new(tag: &str) -> Self {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |elapsed| elapsed.as_nanos());
        let path = std::env::temp_dir().join(format!("deckhand-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
