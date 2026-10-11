use std::{
    sync::{
        Arc,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
    time::Duration,
};

use deckhand_core::{
    Config, Error, ErrorCode, Result,
    ipc::ToWorker,
    protocol::{Call, Reply},
};
use tokio::sync::Mutex;

use crate::link::{Death, Link, WorkerPipes, seconds, stopped_error};

const READY_TIMEOUT: Duration = Duration::from_secs(10);
const EXITED_DURING_STARTUP: &str = "exited during startup; its error output is in the MCP server log";

pub trait Spawn: Send + Sync {
    fn spawn(&self) -> Result<WorkerPipes>;
}

pub struct Supervisor {
    spawner: Box<dyn Spawn>,
    config: Config,
    slot: Mutex<Option<Arc<Link>>>,
    next_id: AtomicU64,
    restarts: Arc<AtomicU32>,
    ready_timeout: Duration,
}

impl Supervisor {
    pub fn new(spawner: Box<dyn Spawn>, config: Config) -> Self {
        Self {
            spawner,
            config,
            slot: Mutex::new(None),
            next_id: AtomicU64::new(1),
            restarts: Arc::new(AtomicU32::new(0)),
            ready_timeout: READY_TIMEOUT,
        }
    }

    pub fn restarts(&self) -> u32 {
        self.restarts.load(Ordering::SeqCst)
    }

    pub async fn call(&self, call: Call, cancelled: impl Future<Output = ()>) -> Result<Reply> {
        let deadline = call.deadline();
        self.call_within(call, deadline, cancelled).await
    }

    async fn call_within(&self, call: Call, deadline: Duration, cancelled: impl Future<Output = ()>) -> Result<Reply> {
        let link = self.link().await?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let tool = call.name();
        let reply = link.register(id)?;
        if let Err(error) = link.send(ToWorker::Call { id, call }) {
            link.forget(id);
            return Err(error);
        }
        tokio::select! {
            outcome = reply => outcome.unwrap_or_else(|_| Err(stopped_error())),
            () = tokio::time::sleep(deadline) => Err(abandon(&link, &Death::Timeout { tool, deadline })),
            () = cancelled => Err(cancel(&link, id)),
        }
    }

    async fn link(&self) -> Result<Arc<Link>> {
        let mut slot = self.slot.lock().await;
        if let Some(link) = slot.as_ref().filter(|link| link.is_alive()) {
            return Ok(Arc::clone(link));
        }
        let link = self.start().await?;
        *slot = Some(Arc::clone(&link));
        Ok(link)
    }

    async fn start(&self) -> Result<Arc<Link>> {
        let pipes = self.spawner.spawn()?;
        let (link, announced) = Link::start(pipes, Arc::clone(&self.restarts))?;
        if link.send(ToWorker::Init { config: Box::new(self.config.clone()) }).is_err() {
            return Err(startup_error(EXITED_DURING_STARTUP));
        }
        match tokio::time::timeout(self.ready_timeout, announced).await {
            Ok(Ok(_version)) => {
                link.mark_started();
                Ok(link)
            }
            Ok(Err(_)) => Err(startup_error(EXITED_DURING_STARTUP)),
            Err(_) => Err(startup_error(&format!("did not start within {}", seconds(self.ready_timeout)))),
        }
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        if let Some(link) = self.slot.get_mut().take() {
            link.stop(&Death::Shutdown);
        }
    }
}

fn abandon(link: &Link, death: &Death) -> Error {
    link.stop(death);
    death.error()
}

fn cancel(link: &Link, id: u64) -> Error {
    link.forget(id);
    let _ = link.send(ToWorker::Cancel { id });
    Error::new(ErrorCode::Cancelled, "the request was cancelled")
}

fn startup_error(what: &str) -> Error {
    Error::new(ErrorCode::TargetNotResponding, format!("the desktop worker {what}"))
}

#[cfg(test)]
mod tests {
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };

    use deckhand_core::protocol::ClipboardReport;

    use super::*;
    use crate::testing::{FakeSpawner, FakeWorker, echo_worker, hang_worker, set_clipboard};

    const LONG: Duration = Duration::from_secs(30);

    fn supervisor(behaviour: impl Fn(usize, FakeWorker) + Send + Sync + 'static) -> (Supervisor, FakeSpawner) {
        let spawner = FakeSpawner::new(behaviour);
        (Supervisor::new(Box::new(spawner.clone()), Config::default()), spawner)
    }

    async fn call(supervisor: &Supervisor, call: Call, deadline: Duration) -> Result<Reply> {
        supervisor.call_within(call, deadline, std::future::pending()).await
    }

    fn clipboard_text(reply: &Result<Reply>) -> Option<String> {
        match reply {
            Ok(Reply::Clipboard(ClipboardReport { text, .. })) => text.clone(),
            _ => None,
        }
    }

    fn assert_restart_error(outcome: &Result<Reply>, fragment: &str) {
        let error = outcome.as_ref().unwrap_err();
        assert_eq!(error.code, ErrorCode::TargetNotResponding);
        assert!(error.message.contains(fragment), "{}", error.message);
        assert!(error.message.ends_with("it was restarted"), "{}", error.message);
    }

    #[tokio::test]
    async fn normal_reply_reaches_the_caller() {
        let (supervisor, spawner) = supervisor(|_, worker| echo_worker(worker));
        let reply = call(&supervisor, set_clipboard("hello"), LONG).await;
        assert_eq!(clipboard_text(&reply).as_deref(), Some("hello"));
        assert_eq!(spawner.spawned(), 1);
        assert_eq!(supervisor.restarts(), 0);
    }

    #[tokio::test]
    async fn worker_errors_pass_through() {
        let (supervisor, _) = supervisor(|_, mut worker| {
            worker.handshake();
            while let Some(ToWorker::Call { id, .. }) = worker.receive() {
                worker.reply(id, Err(Error::not_found("no such window")));
            }
        });
        let error = call(&supervisor, Call::Status, LONG).await.unwrap_err();
        assert_eq!(error, Error::not_found("no such window"));
    }

    #[tokio::test]
    async fn init_carries_the_config() {
        let (sender, configs) = mpsc::channel();
        let (supervisor, _) = supervisor(move |_, mut worker| {
            if let Some(ToWorker::Init { config }) = worker.receive() {
                let _ = sender.send(*config);
            }
        });
        let _ = call(&supervisor, Call::Status, LONG).await;
        assert_eq!(configs.recv_timeout(LONG).unwrap(), Config::default());
    }

    #[tokio::test]
    async fn out_of_order_replies_reach_their_own_callers() {
        let (supervisor, _) = supervisor(|_, mut worker| {
            worker.handshake();
            let (Some(first), Some(second)) = (worker.receive(), worker.receive()) else { return };
            worker.answer(&second);
            worker.answer(&first);
            echo_worker(worker);
        });
        let (a, b) = tokio::join!(
            call(&supervisor, set_clipboard("first"), LONG),
            call(&supervisor, set_clipboard("second"), LONG)
        );
        assert_eq!(clipboard_text(&a).as_deref(), Some("first"));
        assert_eq!(clipboard_text(&b).as_deref(), Some("second"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_calls_all_get_their_replies() {
        let (supervisor, spawner) = supervisor(|_, worker| echo_worker(worker));
        let supervisor = Arc::new(supervisor);
        let mut tasks = tokio::task::JoinSet::new();
        for index in 0..32 {
            let supervisor = Arc::clone(&supervisor);
            tasks.spawn(async move { (index, call(&supervisor, set_clipboard(&format!("call {index}")), LONG).await) });
        }
        let mut answered = 0;
        while let Some(joined) = tasks.join_next().await {
            let (index, reply) = joined.unwrap();
            assert_eq!(clipboard_text(&reply), Some(format!("call {index}")));
            answered += 1;
        }
        assert_eq!(answered, 32);
        assert_eq!(spawner.spawned(), 1);
    }

    #[tokio::test]
    async fn deadline_restarts_the_worker_and_fails_every_pending_call() {
        let (supervisor, spawner) =
            supervisor(|index, worker| if index == 0 { hang_worker(worker) } else { echo_worker(worker) });
        let started = Instant::now();
        let (hung, bystander) = tokio::join!(
            call(&supervisor, Call::Status, Duration::from_millis(200)),
            call(&supervisor, set_clipboard("bystander"), LONG)
        );
        assert!(started.elapsed() < Duration::from_secs(5));
        assert_restart_error(&hung, "did not answer status within 0.2 s");
        assert_restart_error(&bystander, "did not answer status within 0.2 s");
        assert_eq!(supervisor.restarts(), 1);
        let after = call(&supervisor, set_clipboard("again"), LONG).await;
        assert_eq!(clipboard_text(&after).as_deref(), Some("again"));
        assert_eq!(spawner.spawned(), 2);
    }

    #[tokio::test]
    async fn worker_exit_fails_the_call_and_respawns() {
        let (supervisor, spawner) = supervisor(|index, mut worker| {
            if index > 0 {
                return echo_worker(worker);
            }
            worker.handshake();
            let _ = worker.receive();
        });
        let outcome = call(&supervisor, Call::Status, LONG).await;
        assert_restart_error(&outcome, "exited unexpectedly");
        assert_eq!(supervisor.restarts(), 1);
        let after = call(&supervisor, set_clipboard("back"), LONG).await;
        assert_eq!(clipboard_text(&after).as_deref(), Some("back"));
        assert_eq!(spawner.spawned(), 2);
    }

    #[tokio::test]
    async fn malformed_frame_fails_the_call_and_restarts() {
        let (supervisor, _) = supervisor(|_, mut worker| {
            worker.handshake();
            let _ = worker.receive();
            let mut frame = 9u32.to_le_bytes().to_vec();
            frame.extend_from_slice(b"not json!");
            worker.write_raw(&frame);
            hang_worker_after_handshake(worker);
        });
        let outcome = call(&supervisor, Call::Status, LONG).await;
        assert_restart_error(&outcome, "unreadable message");
        assert_eq!(supervisor.restarts(), 1);
    }

    #[tokio::test]
    async fn oversized_frame_header_fails_the_call() {
        let (supervisor, _) = supervisor(|_, mut worker| {
            worker.handshake();
            let _ = worker.receive();
            worker.write_raw(&u32::MAX.to_le_bytes());
            hang_worker_after_handshake(worker);
        });
        let outcome = call(&supervisor, Call::Status, LONG).await;
        assert_restart_error(&outcome, "too large");
    }

    #[tokio::test]
    async fn cancellation_sends_cancel_and_returns_cancelled() {
        let (sender, seen) = mpsc::channel();
        let (supervisor, _) = supervisor(move |_, mut worker| {
            worker.handshake();
            let (Some(ToWorker::Call { id, .. }), Some(ToWorker::Cancel { id: cancelled })) =
                (worker.receive(), worker.receive())
            else {
                return;
            };
            let _ = sender.send((id, cancelled));
            hang_worker_after_handshake(worker);
        });
        let cancelled = tokio::time::sleep(Duration::from_millis(50));
        let error = supervisor.call_within(Call::Status, LONG, cancelled).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::Cancelled);
        let (id, cancelled_id) = seen.recv_timeout(LONG).unwrap();
        assert_eq!(id, cancelled_id);
        assert_eq!(supervisor.restarts(), 0);
    }

    #[tokio::test]
    async fn missing_ready_times_out_without_counting_a_restart() {
        let spawner = FakeSpawner::new(|_, worker| hang_worker_after_handshake(worker));
        let mut supervisor = Supervisor::new(Box::new(spawner), Config::default());
        supervisor.ready_timeout = Duration::from_millis(200);
        let error = call(&supervisor, Call::Status, LONG).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::TargetNotResponding);
        assert_eq!(error.message, "the desktop worker did not start within 0.2 s");
        assert_eq!(supervisor.restarts(), 0);
    }

    #[tokio::test]
    async fn exit_during_startup_is_reported() {
        let (supervisor, spawner) = supervisor(|_, worker| drop(worker));
        let error = call(&supervisor, Call::Status, LONG).await.unwrap_err();
        assert!(error.message.contains("exited during startup"), "{}", error.message);
        assert_eq!(supervisor.restarts(), 0);
        let _ = call(&supervisor, Call::Status, LONG).await;
        assert_eq!(spawner.spawned(), 2);
    }

    #[tokio::test]
    async fn late_reply_after_cancel_is_ignored() {
        let (supervisor, _) = supervisor(|_, mut worker| {
            worker.handshake();
            let Some(ToWorker::Call { id, .. }) = worker.receive() else { return };
            let _ = worker.receive();
            worker.reply(id, Ok(Reply::Clipboard(ClipboardReport { text: None, changed: false })));
            echo_worker_after_handshake(worker);
        });
        let cancelled = tokio::time::sleep(Duration::from_millis(50));
        let first = supervisor.call_within(Call::Status, LONG, cancelled).await;
        assert_eq!(first.unwrap_err().code, ErrorCode::Cancelled);
        let second = call(&supervisor, set_clipboard("next"), LONG).await;
        assert_eq!(clipboard_text(&second).as_deref(), Some("next"));
    }

    fn hang_worker_after_handshake(mut worker: FakeWorker) {
        while worker.receive().is_some() {}
    }

    fn echo_worker_after_handshake(mut worker: FakeWorker) {
        while let Some(message) = worker.receive() {
            worker.answer(&message);
        }
    }
}
