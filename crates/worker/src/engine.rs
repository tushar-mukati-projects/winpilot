use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};

use parking_lot::Mutex;
use winpilot_capture::Capturer;
use winpilot_core::{
    Config, Result, Snapshot, WindowInfo,
    protocol::{Call, Reply, Timing},
};
use winpilot_desktop::{
    Notifier,
    changes::ChangeMonitor,
    overlay::{Overlay, OverlayOptions},
};
use winpilot_uia::{SnapshotOptions, UiaHandle, UiaOptions};

use crate::{
    act::{self, NotifyLimit},
    app_catalog::AppCatalog,
    call_scope::{CallScope, working_deadline},
    cancel::Cancel,
    epoch,
    guard::Guard,
    host::HostProcesses,
    images::ImageStore,
    kill_switch::warn,
    observe::{
        self,
        listen::SpeechSlot,
        record::{self, RecordingSlot},
    },
};

const RECENT_TIMINGS: usize = 8;

pub(crate) struct Engine {
    pub(crate) config: Config,
    pub(crate) uia: UiaHandle,
    pub(crate) capturer: Capturer,
    pub(crate) changes: ChangeMonitor,
    pub(crate) guard: Guard,
    pub(crate) overlay: Option<Arc<Overlay>>,
    pub(crate) notifier: Notifier,
    pub(crate) notify_limit: Mutex<NotifyLimit>,
    pub(crate) images: Mutex<ImageStore>,
    pub(crate) host: HostProcesses,
    pub(crate) apps: AppCatalog,
    pub(crate) recording: Mutex<RecordingSlot>,
    pub(crate) speech: SpeechSlot,
    timings: Mutex<VecDeque<Timing>>,
    started: Instant,
}

impl Engine {
    pub(crate) fn start(config: Config, epoch: u32, agent: &str) -> Result<Self> {
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
            notify_limit: Mutex::new(NotifyLimit::default()),
            images: Mutex::new(ImageStore::starting_at(first_ref)),
            host: HostProcesses::detect(),
            apps: AppCatalog::default(),
            recording: Mutex::new(RecordingSlot::default()),
            speech: SpeechSlot::default(),
            timings: Mutex::new(VecDeque::with_capacity(RECENT_TIMINGS)),
            started: Instant::now(),
        })
    }

    pub(crate) fn handle(&self, call: &Call, cancel: &Cancel, budget: Duration) -> Result<Reply> {
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

    pub(crate) fn snapshot(&self, window: &WindowInfo) -> Result<Snapshot> {
        self.guard.check_window(window)?;
        let window = window.clone();
        let options = self.snapshot_options();
        self.uia.run(move |uia| uia.snapshot(&window, &options))
    }

    pub(crate) fn snapshot_budget(&self) -> Duration {
        Duration::from_millis(self.config.act.snapshot_budget_ms)
    }

    fn snapshot_options(&self) -> SnapshotOptions {
        SnapshotOptions {
            max_elements: self.config.act.max_elements,
            include_offscreen: false,
            budget: self.snapshot_budget(),
        }
    }

    pub(crate) fn rename_agent(&self, agent: &str) {
        if let Some(overlay) = &self.overlay {
            overlay.relabel(self.config.overlay.display_label(agent));
        }
    }

    pub(crate) fn begin_shut_down(&self) {
        self.recording.lock().close();
    }

    pub(crate) fn shut_down(&self) {
        record::shut_down(self);
        self.notifier.dismiss_all();
    }

    pub(crate) fn uptime(&self) -> Duration {
        self.started.elapsed()
    }

    pub(crate) fn recent_timings(&self) -> Vec<Timing> {
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

pub(crate) fn duration_ms(duration: Duration) -> u64 {
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
    Overlay::spawn(options).inspect_err(|error| warn("overlay", error)).ok().map(Arc::new)
}
