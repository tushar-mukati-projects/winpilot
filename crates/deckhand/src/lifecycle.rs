use std::{
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::{Duration, Instant},
};

use deckhand_core::{Config, Error, Result, ipc::ToWorker};
use parking_lot::Mutex;
use tokio::{runtime::Handle, sync::watch};

use crate::{
    backoff::Failure,
    death::{Death, seconds},
    link::{Announced, DeathHook, Link, WorkerPipes},
};

const EXITED_DURING_STARTUP: &str = "it exited during startup; its error output is in the MCP server log";

pub trait Spawn: Send + Sync {
    fn spawn(&self) -> Result<WorkerPipes>;
}

type Outcome = Option<Result<Arc<Link>>>;
pub type Attempt = watch::Receiver<Outcome>;

pub enum Next {
    Use(Arc<Link>),
    Await(Attempt),
    Fail(Error),
}

enum Phase {
    Idle,
    Starting(Attempt),
    Running(Arc<Link>),
    Broken(Failure),
    ShutDown,
}

struct Slot {
    phase: Phase,
    failures: u32,
}

impl Slot {
    fn holds(&self, link: &Link) -> bool {
        matches!(&self.phase, Phase::Running(running) if std::ptr::eq(Arc::as_ptr(running), link))
    }
}

pub struct Lifecycle {
    spawner: Box<dyn Spawn>,
    config: Config,
    ready_timeout: Duration,
    slot: Mutex<Slot>,
    restarts: AtomicU32,
    epochs: AtomicU32,
    agent: Mutex<String>,
}

impl Lifecycle {
    pub fn new(spawner: Box<dyn Spawn>, config: Config, ready_timeout: Duration) -> Self {
        Self {
            spawner,
            config,
            ready_timeout,
            slot: Mutex::new(Slot { phase: Phase::Idle, failures: 0 }),
            restarts: AtomicU32::new(0),
            epochs: AtomicU32::new(0),
            agent: Mutex::new(String::new()),
        }
    }

    pub fn set_agent(&self, name: &str) {
        let name = name.trim();
        {
            let mut agent = self.agent.lock();
            if *agent == name {
                return;
            }
            name.clone_into(&mut agent);
        }
        if let Phase::Running(link) = &self.slot.lock().phase {
            let _ = link.send(ToWorker::Agent { name: name.to_string() });
        }
    }

    pub fn restarts(&self) -> u32 {
        self.restarts.load(Ordering::SeqCst)
    }

    pub fn next(self: &Arc<Self>) -> Next {
        let Ok(runtime) = Handle::try_current() else {
            return Next::Fail(Error::internal("the worker supervisor was called outside the async runtime"));
        };
        let now = Instant::now();
        let mut slot = self.slot.lock();
        match &slot.phase {
            Phase::Running(link) if link.is_alive() => return Next::Use(Arc::clone(link)),
            Phase::Starting(attempt) if attempt.has_changed().is_ok() => return Next::Await(attempt.clone()),
            Phase::Broken(failure) if !failure.is_over(now) => return Next::Fail(failure.error(now)),
            Phase::ShutDown => return Next::Fail(Death::Shutdown.error()),
            Phase::Idle | Phase::Starting(_) | Phase::Running(_) | Phase::Broken(_) => {}
        }
        Next::Await(self.begin_start(&mut slot, &runtime))
    }

    pub fn shut_down(&self) {
        let previous = std::mem::replace(&mut self.slot.lock().phase, Phase::ShutDown);
        if let Phase::Running(link) = previous {
            link.stop(&Death::Shutdown);
        }
    }

    fn begin_start(self: &Arc<Self>, slot: &mut Slot, runtime: &Handle) -> Attempt {
        let (publisher, attempt) = watch::channel(None);
        slot.phase = Phase::Starting(attempt.clone());
        let lifecycle = Arc::clone(self);
        let task_runtime = runtime.clone();
        runtime.spawn(async move { lifecycle.run_start(task_runtime, publisher).await });
        attempt
    }

    async fn run_start(self: Arc<Self>, runtime: Handle, publisher: watch::Sender<Outcome>) {
        let started = self.start(&runtime).await;
        let outcome = self.finish_start(started);
        publisher.send_replace(Some(outcome));
    }

    async fn start(self: &Arc<Self>, runtime: &Handle) -> Result<Arc<Link>, String> {
        let pipes = self.spawner.spawn().map_err(|error| error.message)?;
        let (link, announced) = Link::start(pipes, self.death_hook(runtime)).map_err(|error| error.message)?;
        let epoch = self.epochs.fetch_add(1, Ordering::SeqCst);
        let agent = self.agent.lock().clone();
        if link.send(ToWorker::Init { config: Box::new(self.config.clone()), epoch, agent: agent.clone() }).is_err() {
            return Err(EXITED_DURING_STARTUP.to_string());
        }
        let worker = match tokio::time::timeout(self.ready_timeout, announced).await {
            Ok(Ok(worker)) => worker,
            Ok(Err(_)) => return Err(EXITED_DURING_STARTUP.to_string()),
            Err(_) => return Err(format!("it did not report ready within {}", seconds(self.ready_timeout))),
        };
        check_build(&worker)?;
        let current = self.agent.lock().clone();
        if current != agent {
            let _ = link.send(ToWorker::Agent { name: current });
        }
        Ok(link)
    }

    fn finish_start(&self, started: Result<Arc<Link>, String>) -> Result<Arc<Link>> {
        let now = Instant::now();
        let mut slot = self.slot.lock();
        if matches!(slot.phase, Phase::ShutDown) {
            drop(slot);
            drop(started);
            return Err(Death::Shutdown.error());
        }
        match started.and_then(still_alive) {
            Ok(link) => {
                link.mark_started();
                slot.failures = 0;
                slot.phase = Phase::Running(Arc::clone(&link));
                Ok(link)
            }
            Err(cause) => {
                slot.failures = slot.failures.saturating_add(1);
                let failure = Failure::new(cause, slot.failures, now);
                let error = failure.error(now);
                slot.phase = Phase::Broken(failure);
                Err(error)
            }
        }
    }

    fn death_hook(self: &Arc<Self>, runtime: &Handle) -> DeathHook {
        let lifecycle = Arc::downgrade(self);
        let runtime = runtime.clone();
        Box::new(move |link, death, in_use| {
            if let Some(lifecycle) = lifecycle.upgrade() {
                lifecycle.after_death(link, death, in_use, &runtime);
            }
        })
    }

    fn after_death(self: &Arc<Self>, link: &Link, death: &Death, in_use: bool, runtime: &Handle) {
        if !death.counts_as_restart() {
            return;
        }
        if link.is_started() {
            self.restarts.fetch_add(1, Ordering::SeqCst);
        }
        let mut slot = self.slot.lock();
        if !slot.holds(link) {
            return;
        }
        if in_use {
            self.begin_start(&mut slot, runtime);
        } else {
            slot.phase = Phase::Idle;
        }
    }
}

fn still_alive(link: Arc<Link>) -> Result<Arc<Link>, String> {
    if link.is_alive() { Ok(link) } else { Err(EXITED_DURING_STARTUP.to_string()) }
}

fn check_build(worker: &Announced) -> Result<(), String> {
    if worker.version == crate::VERSION && worker.build == crate::BUILD_ID {
        return Ok(());
    }
    Err(format!(
        "a newer Deckhand was installed (worker {} build {}, this server {} build {}); restart Claude Code to load it",
        worker.version,
        shown(&worker.build),
        crate::VERSION,
        crate::BUILD_ID
    ))
}

fn shown(build: &str) -> &str {
    if build.is_empty() { "unknown" } else { build }
}
