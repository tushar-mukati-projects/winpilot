use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};

use deckhand_capture::Capturer;
use deckhand_core::{
    Config, Result, Snapshot, WindowInfo,
    protocol::{Call, Reply, Timing},
};
use deckhand_desktop::{
    Notifier,
    changes::ChangeMonitor,
    overlay::{Overlay, OverlayOptions},
};
use deckhand_uia::{SnapshotOptions, UiaHandle, UiaOptions};
use parking_lot::Mutex;

use crate::{
    act,
    app_catalog::AppCatalog,
    call_scope::{CallScope, working_deadline},
    cancel::Cancel,
    epoch,
    guard::Guard,
    host::HostProcesses,
    images::ImageStore,
    observe::{
        self,
        record::{self, RecordingSlot},
    },
};

const RECENT_TIMINGS: usize = 8;

pub struct Engine {
    pub config: Config,
    pub uia: UiaHandle,
    pub capturer: Capturer,
    pub changes: ChangeMonitor,
    pub guard: Guard,
    pub overlay: Option<Arc<Overlay>>,
    pub notifier: Notifier,
    pub images: Mutex<ImageStore>,
    pub host: HostProcesses,
    pub apps: AppCatalog,
    pub recording: Mutex<RecordingSlot>,
    timings: Mutex<VecDeque<Timing>>,
    started: Instant,
}

impl Engine {
    pub fn start(config: Config, epoch: u32, agent: &str) -> Result<Self> {
        let first_ref = epoch::first_ref(epoch);
        let uia = UiaHandle::spawn(UiaOptions { first_ref, ..UiaOptions::default() })?;
        let capturer = Capturer::spawn()?;
        let changes = ChangeMonitor::spawn()?;
        let overlay = if config.overlay.enabled { spawn_overlay(&config, agent) } else { None };
        let guard = Guard::start(&config.guard, overlay.clone())?;
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
            apps: AppCatalog::default(),
            recording: Mutex::new(RecordingSlot::default()),
            timings: Mutex::new(VecDeque::with_capacity(RECENT_TIMINGS)),
            started: Instant::now(),
        })
    }

    pub fn handle(&self, call: &Call, cancel: &Cancel, budget: Duration) -> Result<Reply> {
        let started = Instant::now();
        let label = call.label();
        let scope = CallScope::new(cancel.clone(), working_deadline(started, budget));
        let result = self.dispatch(call, &scope);
        self.record(label, started.elapsed(), result.is_ok());
        result
    }

    fn dispatch(&self, call: &Call, scope: &CallScope) -> Result<Reply> {
        match call {
            Call::Observe(args) => observe::run(self, args, scope),
            Call::Act(args) => act::run(self, args, scope).map(Reply::Act),
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

    pub fn rename_agent(&self, agent: &str) {
        if let Some(overlay) = &self.overlay {
            overlay.relabel(self.config.overlay.display_label(agent));
        }
    }

    pub fn begin_shut_down(&self) {
        self.recording.lock().close();
    }

    pub fn shut_down(&self) {
        record::shut_down(self);
        self.notifier.dismiss_all();
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

fn spawn_overlay(config: &Config, agent: &str) -> Option<Arc<Overlay>> {
    let hotkey = config.guard.kill_hotkey.trim();
    let options = OverlayOptions {
        animations: config.overlay.animations,
        capturable: config.overlay.capturable,
        kill_hotkey: (!hotkey.is_empty()).then(|| hotkey.to_string()),
        label: config.overlay.display_label(agent),
        palette: config.overlay.palette(),
    };
    let spawned =
        Overlay::spawn(options).inspect_err(|error| eprintln!("deckhand worker: overlay unavailable: {error}"));
    spawned.ok().map(Arc::new)
}
