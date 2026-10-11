use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use deckhand_capture::Capturer;
use deckhand_core::{
    Config, Result, Snapshot, WindowInfo,
    protocol::{Call, Reply, Timing},
};
use deckhand_desktop::{Notifier, changes::ChangeMonitor, overlay::Overlay};
use deckhand_uia::{SnapshotOptions, UiaHandle, UiaOptions};
use parking_lot::Mutex;

use crate::{
    act,
    call_scope::{CallScope, working_deadline},
    cancel::Cancel,
    clipboard_tool, epoch, find,
    guard::Guard,
    host::HostProcesses,
    images::ImageStore,
    notify_tool, observe, pick_file, read_text, screenshot, status, wait, window_tool,
};

const RECENT_TIMINGS: usize = 8;

pub struct Engine {
    pub config: Config,
    pub uia: UiaHandle,
    pub capturer: Capturer,
    pub changes: ChangeMonitor,
    pub guard: Guard,
    pub overlay: Option<Overlay>,
    pub notifier: Notifier,
    pub images: Mutex<ImageStore>,
    pub host: HostProcesses,
    pub apps: window_tool::AppCatalog,
    timings: Mutex<VecDeque<Timing>>,
    started: Instant,
}

impl Engine {
    pub fn start(config: Config, epoch: u32) -> Result<Self> {
        let first_ref = epoch::first_ref(epoch);
        let uia = UiaHandle::spawn(UiaOptions { first_ref, ..UiaOptions::default() })?;
        let capturer = Capturer::spawn()?;
        let changes = ChangeMonitor::spawn()?;
        let guard = Guard::start(&config.guard)?;
        let overlay = if config.overlay.enabled { spawn_overlay() } else { None };
        Ok(Self {
            config,
            uia,
            capturer,
            changes,
            guard,
            overlay,
            notifier: Notifier::new(),
            images: Mutex::new(ImageStore::starting_at(first_ref)),
            host: HostProcesses::detect(),
            apps: window_tool::AppCatalog::default(),
            timings: Mutex::new(VecDeque::with_capacity(RECENT_TIMINGS)),
            started: Instant::now(),
        })
    }

    pub fn handle(&self, call: Call, cancel: &Cancel) -> Result<Reply> {
        let started = Instant::now();
        let tool = call.name();
        let scope = CallScope::new(cancel.clone(), working_deadline(started, call.deadline()));
        let result = self.dispatch(call, &scope);
        self.record(tool, started.elapsed(), result.is_ok());
        result
    }

    fn dispatch(&self, call: Call, scope: &CallScope) -> Result<Reply> {
        match call {
            Call::Observe(args) => observe::run(self, &args).map(Reply::Observe),
            Call::Find(args) => find::run(self, &args, scope).map(Reply::Find),
            Call::Screenshot(args) => screenshot::run(self, &args, scope).map(Reply::Screenshot),
            Call::Act(args) => act::run(self, &args, scope).map(Reply::Act),
            Call::WaitFor(condition) => wait::run(self, &condition, scope).map(Reply::WaitFor),
            Call::Windows(args) => window_tool::run(self, &args, scope).map(Reply::Windows),
            Call::PickFile(args) => pick_file::run(self, &args, scope).map(Reply::PickFile),
            Call::ReadText(args) => read_text::run(self, &args, scope).map(Reply::ReadText),
            Call::Clipboard(args) => clipboard_tool::run(self, &args, scope).map(Reply::Clipboard),
            Call::Notify(args) => notify_tool::run(self, &args, scope).map(Reply::Notify),
            Call::Status => Ok(Reply::Status(status::report(self))),
        }
    }

    pub fn snapshot(&self, window: &WindowInfo) -> Result<Snapshot> {
        self.guard.check_window(window)?;
        let window = window.clone();
        let options = self.snapshot_options();
        self.uia.run(move |uia| uia.snapshot(&window, &options))
    }

    pub fn snapshot_budget(&self) -> Duration {
        Duration::from_millis(self.config.act.snapshot_budget_ms)
    }

    fn snapshot_options(&self) -> SnapshotOptions {
        SnapshotOptions {
            max_elements: self.config.act.max_elements,
            include_offscreen: false,
            budget: self.snapshot_budget(),
        }
    }

    pub fn uptime(&self) -> Duration {
        self.started.elapsed()
    }

    pub fn recent_timings(&self) -> Vec<Timing> {
        self.timings.lock().iter().cloned().collect()
    }

    fn record(&self, tool: &str, elapsed: Duration, ok: bool) {
        let mut timings = self.timings.lock();
        if timings.len() == RECENT_TIMINGS {
            timings.pop_front();
        }
        timings.push_back(Timing { tool: tool.to_string(), ms: duration_ms(elapsed), ok });
    }
}

pub fn duration_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn spawn_overlay() -> Option<Overlay> {
    Overlay::spawn().inspect_err(|error| eprintln!("deckhand worker: overlay unavailable: {error}")).ok()
}
