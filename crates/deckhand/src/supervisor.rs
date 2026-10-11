use std::{
    pin::{Pin, pin},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use deckhand_core::{
    Config, Error, ErrorCode, Result,
    ipc::ToWorker,
    protocol::{Call, Reply},
};
use tokio::sync::oneshot::error::RecvError;

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
        let budget = call.deadline();
        self.call_within(call, budget, cancelled).await
    }

    async fn call_within(&self, call: Call, budget: Duration, cancelled: impl Future<Output = ()>) -> Result<Reply> {
        let deadline = Deadline::from_now(call.name(), budget);
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
        if let Err(error) = link.send(ToWorker::Call { id, call }) {
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
    Error::new(ErrorCode::Cancelled, "the request was cancelled")
}

#[cfg(test)]
mod tests {
    use std::{
        sync::mpsc,
        time::{Duration, Instant},
    };

    use deckhand_core::protocol::{ClipboardArgs, ClipboardReport};
    use tokio::sync::oneshot;

    use super::*;
    use crate::{
        death::seconds,
        testing::{
            FakeSpawner, FakeWorker, drain, echo, echo_after_handshake, echo_worker, hang_worker, set_clipboard,
        },
    };

    const LONG: Duration = Duration::from_secs(30);

    fn supervisor(behaviour: impl Fn(usize, FakeWorker) + Send + Sync + 'static) -> (Supervisor, FakeSpawner) {
        let spawner = FakeSpawner::new(behaviour);
        (Supervisor::new(Box::new(spawner.clone()), Config::default()), spawner)
    }

    async fn call(supervisor: &Supervisor, call: Call, deadline: Duration) -> Result<Reply> {
        supervisor.call_within(call, deadline, std::future::pending()).await
    }

    async fn timed_call(supervisor: &Supervisor, budget: Duration, started: Instant) -> (Result<Reply>, Duration) {
        let outcome = call(supervisor, Call::Status, budget).await;
        (outcome, started.elapsed())
    }

    async fn spawns_reach(spawner: &FakeSpawner, count: usize) -> bool {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(5) {
            if spawner.spawned() >= count {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        false
    }

    fn clipboard_text(reply: &Result<Reply>) -> Option<String> {
        match reply {
            Ok(Reply::Clipboard(ClipboardReport { text, .. })) => text.clone(),
            _ => None,
        }
    }

    fn call_text(call: &Call) -> Option<String> {
        match call {
            Call::Clipboard(ClipboardArgs { text, .. }) => text.clone(),
            _ => None,
        }
    }

    fn assert_restart_error(outcome: &Result<Reply>, fragment: &str) {
        let error = outcome.as_ref().unwrap_err();
        assert_eq!(error.code, ErrorCode::TargetNotResponding);
        assert!(error.message.contains(fragment), "{}", error.message);
        assert!(error.message.ends_with("it was restarted"), "{}", error.message);
    }

    fn recording(sender: mpsc::Sender<Call>, delay: Duration) -> impl Fn(usize, FakeWorker) + Send + Sync + 'static {
        move |_, mut worker| {
            std::thread::sleep(delay);
            if !worker.handshake() {
                return;
            }
            while let Some(message) = worker.receive() {
                if let ToWorker::Call { call, .. } = &message {
                    let _ = sender.send(call.clone());
                }
                worker.answer(&message);
            }
        }
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
            if let Some(ToWorker::Init { config, .. }) = worker.receive() {
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
            echo_after_handshake(worker);
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
    async fn startup_hang_lets_each_call_finish_near_its_own_deadline() {
        let (supervisor, spawner) = supervisor(|_, worker| drain(worker));
        let started = Instant::now();
        let budgets = [Duration::from_millis(200), Duration::from_millis(400), Duration::from_millis(600)];
        let (a, b, c) = tokio::join!(
            timed_call(&supervisor, budgets[0], started),
            timed_call(&supervisor, budgets[1], started),
            timed_call(&supervisor, budgets[2], started)
        );
        for ((outcome, elapsed), budget) in [a, b, c].into_iter().zip(budgets) {
            let error = outcome.unwrap_err();
            assert_eq!(error.code, ErrorCode::TargetNotResponding);
            let expected = format!("still starting when status reached its {} deadline", seconds(budget));
            assert!(error.message.contains(&expected), "{}", error.message);
            assert!(elapsed >= budget, "{elapsed:?} < {budget:?}");
            assert!(elapsed < budget + Duration::from_millis(300), "{elapsed:?} is far past {budget:?}");
        }
        assert_eq!(spawner.spawned(), 1);
    }

    #[tokio::test]
    async fn concurrent_callers_share_one_failed_startup() {
        let (supervisor, spawner) = supervisor(|_, mut worker| {
            let _ = worker.receive();
            std::thread::sleep(Duration::from_millis(100));
        });
        let (a, b, c) = tokio::join!(
            call(&supervisor, Call::Status, LONG),
            call(&supervisor, set_clipboard("b"), LONG),
            call(&supervisor, set_clipboard("c"), LONG)
        );
        for outcome in [a, b, c] {
            let error = outcome.unwrap_err();
            assert!(error.message.starts_with("the desktop worker failed to start: it exited"), "{}", error.message);
        }
        assert_eq!(spawner.spawned(), 1);
    }

    #[tokio::test]
    async fn concurrent_callers_share_one_slow_startup() {
        let (supervisor, spawner) = supervisor(|_, worker| {
            std::thread::sleep(Duration::from_millis(150));
            echo_worker(worker);
        });
        let (a, b, c) = tokio::join!(
            call(&supervisor, set_clipboard("a"), LONG),
            call(&supervisor, set_clipboard("b"), LONG),
            call(&supervisor, set_clipboard("c"), LONG)
        );
        assert_eq!(
            [clipboard_text(&a), clipboard_text(&b), clipboard_text(&c)].map(|text| text.unwrap()),
            ["a", "b", "c"]
        );
        assert_eq!(spawner.spawned(), 1);
    }

    #[tokio::test]
    async fn failed_startup_backs_off_and_fails_fast_in_the_window() {
        let (supervisor, spawner) = supervisor(|_, worker| drop(worker));
        let first = call(&supervisor, Call::Status, LONG).await.unwrap_err();
        assert_eq!(first.code, ErrorCode::TargetNotResponding);
        assert_eq!(
            first.message,
            "the desktop worker failed to start: it exited during startup; its error output is in the MCP server log; \
             retrying in 0.5 s"
        );
        let started = Instant::now();
        let fast = call(&supervisor, Call::Status, LONG).await.unwrap_err();
        assert!(started.elapsed() < Duration::from_millis(100));
        assert!(fast.message.starts_with("the desktop worker failed to start: it exited during startup"));
        assert!(fast.message.contains("; retrying in 0."), "{}", fast.message);
        assert_eq!(spawner.spawned(), 1);
        tokio::time::sleep(Duration::from_millis(600)).await;
        let retried = call(&supervisor, Call::Status, LONG).await.unwrap_err();
        assert!(retried.message.ends_with("; retrying in 1 s"), "{}", retried.message);
        assert_eq!(spawner.spawned(), 2);
        assert_eq!(supervisor.restarts(), 0);
    }

    #[tokio::test]
    async fn missing_ready_times_out_without_counting_a_restart() {
        let spawner = FakeSpawner::new(|_, worker| drain(worker));
        let supervisor =
            Supervisor::with_ready_timeout(Box::new(spawner), Config::default(), Duration::from_millis(200));
        let error = call(&supervisor, Call::Status, LONG).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::TargetNotResponding);
        assert_eq!(
            error.message,
            "the desktop worker failed to start: it did not report ready within 0.2 s; retrying in 0.5 s"
        );
        assert_eq!(supervisor.restarts(), 0);
    }

    #[tokio::test]
    async fn worker_version_mismatch_fails_startup() {
        let (supervisor, _) = supervisor(|_, mut worker| {
            worker.handshake_as("0.0.0-old");
            drain(worker);
        });
        let error = call(&supervisor, Call::Status, LONG).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::TargetNotResponding);
        let expected = format!(
            "worker version 0.0.0-old does not match server version {}; restart Claude Code to load the new \
             Deckhand build",
            crate::VERSION
        );
        assert!(error.message.contains(&expected), "{}", error.message);
        assert_eq!(supervisor.restarts(), 0);
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
        assert!(spawns_reach(&spawner, 2).await, "the replacement worker was not started eagerly");
        let after = call(&supervisor, set_clipboard("again"), LONG).await;
        assert_eq!(clipboard_text(&after).as_deref(), Some("again"));
        assert_eq!(spawner.spawned(), 2);
    }

    #[tokio::test]
    async fn worker_exit_fails_the_call_and_respawns_eagerly() {
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
        assert!(spawns_reach(&spawner, 2).await, "the replacement worker was not started eagerly");
        let after = call(&supervisor, set_clipboard("back"), LONG).await;
        assert_eq!(clipboard_text(&after).as_deref(), Some("back"));
        assert_eq!(spawner.spawned(), 2);
    }

    #[tokio::test]
    async fn idle_crash_of_a_used_worker_restarts_it_without_a_call() {
        let (supervisor, spawner) = supervisor(|index, mut worker| {
            if index > 0 {
                return echo_worker(worker);
            }
            worker.handshake();
            let Some(message) = worker.receive() else { return };
            worker.answer(&message);
            std::thread::sleep(Duration::from_millis(50));
        });
        let first = call(&supervisor, set_clipboard("one"), LONG).await;
        assert_eq!(clipboard_text(&first).as_deref(), Some("one"));
        assert!(spawns_reach(&spawner, 2).await, "the replacement worker was not started eagerly");
        assert_eq!(supervisor.restarts(), 1);
        let second = call(&supervisor, set_clipboard("two"), LONG).await;
        assert_eq!(clipboard_text(&second).as_deref(), Some("two"));
        assert_eq!(spawner.spawned(), 2);
    }

    #[tokio::test]
    async fn idle_crash_of_an_unused_worker_waits_for_the_next_call() {
        let (supervisor, spawner) = supervisor(|index, mut worker| {
            if index > 0 {
                return echo_worker(worker);
            }
            std::thread::sleep(Duration::from_millis(150));
            worker.handshake();
            std::thread::sleep(Duration::from_millis(100));
        });
        let early = call(&supervisor, Call::Status, Duration::from_millis(50)).await.unwrap_err();
        assert!(early.message.contains("still starting"), "{}", early.message);
        tokio::time::sleep(Duration::from_millis(600)).await;
        assert_eq!(spawner.spawned(), 1);
        assert_eq!(supervisor.restarts(), 1);
        let later = call(&supervisor, set_clipboard("later"), LONG).await;
        assert_eq!(clipboard_text(&later).as_deref(), Some("later"));
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
            drain(worker);
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
            drain(worker);
        });
        let outcome = call(&supervisor, Call::Status, LONG).await;
        assert_restart_error(&outcome, "too large");
    }

    #[tokio::test]
    async fn cancellation_sends_cancel_and_returns_cancelled() {
        let (sender, seen) = mpsc::channel();
        let (supervisor, _) = supervisor(move |_, mut worker| {
            worker.handshake();
            let Some(warm_up) = worker.receive() else { return };
            worker.answer(&warm_up);
            let (Some(ToWorker::Call { id, .. }), Some(ToWorker::Cancel { id: cancelled })) =
                (worker.receive(), worker.receive())
            else {
                return;
            };
            let _ = sender.send((id, cancelled));
            drain(worker);
        });
        call(&supervisor, set_clipboard("warm"), LONG).await.unwrap();
        let cancelled = tokio::time::sleep(Duration::from_millis(50));
        let error = supervisor.call_within(Call::Status, LONG, cancelled).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::Cancelled);
        let (id, cancelled_id) = seen.recv_timeout(LONG).unwrap();
        assert_eq!(id, cancelled_id);
        assert_eq!(supervisor.restarts(), 0);
    }

    #[tokio::test]
    async fn call_cancelled_during_startup_is_never_sent() {
        let (sender, sent) = mpsc::channel();
        let (supervisor, spawner) = supervisor(recording(sender, Duration::from_millis(300)));
        let early_cancel = tokio::time::sleep(Duration::from_millis(50));
        let error = supervisor.call_within(set_clipboard("cancelled"), LONG, early_cancel).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::Cancelled);
        let reply = call(&supervisor, set_clipboard("sent"), LONG).await;
        assert_eq!(clipboard_text(&reply).as_deref(), Some("sent"));
        let texts: Vec<Option<String>> = sent.try_iter().map(|call| call_text(&call)).collect();
        assert_eq!(texts, [Some("sent".to_string())]);
        assert_eq!(spawner.spawned(), 1);
    }

    #[tokio::test]
    async fn call_cancelled_before_sending_is_never_sent() {
        let (sender, sent) = mpsc::channel();
        let (supervisor, _) = supervisor(recording(sender, Duration::ZERO));
        call(&supervisor, set_clipboard("warm"), LONG).await.unwrap();
        let error = supervisor.call_within(set_clipboard("cancelled"), LONG, std::future::ready(())).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::Cancelled);
        call(&supervisor, set_clipboard("after"), LONG).await.unwrap();
        let texts: Vec<Option<String>> = sent.try_iter().map(|call| call_text(&call)).collect();
        assert_eq!(texts, [Some("warm".to_string()), Some("after".to_string())]);
    }

    #[tokio::test]
    async fn late_reply_after_cancel_is_ignored() {
        let (supervisor, _) = supervisor(|_, mut worker| {
            worker.handshake();
            let Some(warm_up) = worker.receive() else { return };
            worker.answer(&warm_up);
            let Some(ToWorker::Call { id, .. }) = worker.receive() else { return };
            let _ = worker.receive();
            worker.reply(id, Ok(Reply::Clipboard(ClipboardReport { text: None, changed: false })));
            echo_after_handshake(worker);
        });
        call(&supervisor, set_clipboard("warm"), LONG).await.unwrap();
        let cancelled = tokio::time::sleep(Duration::from_millis(50));
        let first = supervisor.call_within(Call::Status, LONG, cancelled).await;
        assert_eq!(first.unwrap_err().code, ErrorCode::Cancelled);
        let second = call(&supervisor, set_clipboard("next"), LONG).await;
        assert_eq!(clipboard_text(&second).as_deref(), Some("next"));
    }

    #[tokio::test]
    async fn reply_wins_when_it_and_the_deadline_are_both_ready() {
        for _ in 0..64 {
            let (sender, mut reply) = oneshot::channel::<Result<Reply>>();
            let _ = sender.send(Ok(echo(&Call::Status)));
            let settled = first_of(&mut reply, pin!(std::future::ready(())), std::future::ready(())).await;
            assert!(matches!(settled, Settled::Replied(Ok(Ok(Reply::Status(_))))));
        }
    }

    #[tokio::test]
    async fn cancellation_wins_over_an_expired_deadline() {
        for _ in 0..64 {
            let (_sender, mut reply) = oneshot::channel::<Result<Reply>>();
            let settled = first_of(&mut reply, pin!(std::future::ready(())), std::future::ready(())).await;
            assert!(matches!(settled, Settled::Cancelled));
        }
    }

    #[tokio::test]
    async fn shutdown_closes_the_worker_input_so_it_can_finish() {
        let (finished, saw_end) = mpsc::channel();
        let (supervisor, _) = supervisor(move |_, mut worker| {
            if worker.handshake() {
                echo_after_handshake(worker);
                let _ = finished.send(());
            }
        });
        call(&supervisor, set_clipboard("warm"), LONG).await.unwrap();
        supervisor.shutdown_handle().shut_down();
        saw_end.recv_timeout(Duration::from_secs(5)).unwrap();
        let refused = call(&supervisor, Call::Status, LONG).await.unwrap_err();
        assert_eq!(refused.code, ErrorCode::Cancelled);
    }

    #[tokio::test]
    async fn shutdown_fails_pending_calls_and_refuses_new_ones() {
        let (received, seen) = mpsc::channel();
        let (supervisor, _) = supervisor(move |_, mut worker| {
            worker.handshake();
            if worker.receive().is_some() {
                let _ = received.send(());
            }
            drain(worker);
        });
        let shut_down = async {
            while seen.try_recv().is_err() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            supervisor.lifecycle.shut_down();
        };
        let (outcome, ()) = tokio::join!(call(&supervisor, Call::Status, LONG), shut_down);
        assert_eq!(outcome.unwrap_err().code, ErrorCode::Cancelled);
        assert_eq!(supervisor.restarts(), 0);
        let refused = call(&supervisor, Call::Status, LONG).await.unwrap_err();
        assert_eq!(refused.code, ErrorCode::Cancelled);
    }
}
