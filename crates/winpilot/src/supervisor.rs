#[cfg(test)]
mod supervisor_tests;

use std::{
    pin::{Pin, pin},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use tokio::sync::oneshot::error::RecvError;
use winpilot_core::{
    Config, Error, Result,
    ipc::ToWorker,
    millis,
    protocol::{Call, Reply},
};

use crate::{
    deadline::Deadline,
    death::stopped_error,
    lifecycle::{Attempt, Lifecycle, Next, Spawn},
    link::Link,
};

const READY_TIMEOUT: Duration = Duration::from_secs(10);

pub struct Supervisor {
    lifecycle: Arc<Lifecycle>,
    next_id: AtomicU64,
}

pub struct ShutdownHandle(Arc<Lifecycle>);

enum Settled<T> {
    Replied(T),
    Cancelled,
    Expired,
}

impl Supervisor {
    pub fn new(spawner: Box<dyn Spawn>, config: Config) -> Self {
        Self::with_ready_timeout(spawner, config, READY_TIMEOUT)
    }

    fn with_ready_timeout(spawner: Box<dyn Spawn>, config: Config, ready_timeout: Duration) -> Self {
        Self { lifecycle: Arc::new(Lifecycle::new(spawner, config, ready_timeout)), next_id: AtomicU64::new(1) }
    }

    pub fn restarts(&self) -> u32 {
        self.lifecycle.restarts()
    }

    pub fn shutdown_handle(&self) -> ShutdownHandle {
        ShutdownHandle(Arc::clone(&self.lifecycle))
    }

    pub fn set_agent(&self, name: &str) {
        self.lifecycle.set_agent(name);
    }

    pub async fn call(&self, call: Call, cancelled: impl Future<Output = ()>) -> Result<Reply> {
        let budget = call.deadline_with_glide(self.lifecycle.default_glide_ms());
        self.call_within(call, budget, cancelled).await
    }

    async fn call_within(&self, call: Call, budget: Duration, cancelled: impl Future<Output = ()>) -> Result<Reply> {
        let deadline = Deadline::from_now(call.label(), budget);
        let mut cancelled = pin!(cancelled);
        let link = tokio::select! {
            biased;
            link = self.link() => link?,
            () = cancelled.as_mut() => return Err(cancelled_error()),
            () = deadline.expired() => return Err(deadline.still_starting()),
        };
        if is_ready(cancelled.as_mut()).await {
            return Err(cancelled_error());
        }
        if deadline.has_passed() {
            return Err(deadline.still_starting());
        }
        self.send_and_wait(&link, call, &deadline, cancelled).await
    }

    async fn send_and_wait(
        &self,
        link: &Link,
        call: Call,
        deadline: &Deadline,
        cancelled: Pin<&mut impl Future<Output = ()>>,
    ) -> Result<Reply> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut reply = link.register(id)?;
        let deadline_ms = millis(deadline.remaining());
        if let Err(error) = link.send(ToWorker::Call { id, call, deadline_ms }) {
            link.forget(id);
            return Err(error);
        }
        match first_of(&mut reply, cancelled, deadline.expired()).await {
            Settled::Replied(outcome) => flatten(outcome),
            Settled::Cancelled => Err(cancel(link, id)),
            Settled::Expired => {
                link.stop(&deadline.death());
                flatten(reply.await)
            }
        }
    }

    async fn link(&self) -> Result<Arc<Link>> {
        match self.lifecycle.next() {
            Next::Use(link) => Ok(link),
            Next::Fail(error) => Err(error),
            Next::Await(attempt) => outcome_of(attempt).await,
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        self.lifecycle.shut_down();
    }
}

impl ShutdownHandle {
    pub fn shut_down(&self) {
        self.0.shut_down();
    }
}

async fn outcome_of(mut attempt: Attempt) -> Result<Arc<Link>> {
    let Ok(outcome) = attempt.wait_for(Option::is_some).await else { return Err(stopped_error()) };
    Option::clone(&outcome).unwrap_or_else(|| Err(stopped_error()))
}

async fn first_of<T>(
    reply: impl Future<Output = T>,
    cancelled: Pin<&mut impl Future<Output = ()>>,
    expired: impl Future<Output = ()>,
) -> Settled<T> {
    tokio::select! {
        biased;
        outcome = reply => Settled::Replied(outcome),
        () = cancelled => Settled::Cancelled,
        () = expired => Settled::Expired,
    }
}

async fn is_ready(future: Pin<&mut impl Future<Output = ()>>) -> bool {
    tokio::select! {
        biased;
        () = future => true,
        () = std::future::ready(()) => false,
    }
}

fn flatten(outcome: Result<Result<Reply>, RecvError>) -> Result<Reply> {
    outcome.unwrap_or_else(|_| Err(stopped_error()))
}

fn cancel(link: &Link, id: u64) -> Error {
    link.forget(id);
    let _ = link.send(ToWorker::Cancel { id });
    cancelled_error()
}

fn cancelled_error() -> Error {
    Error::cancelled("the request was cancelled")
}
