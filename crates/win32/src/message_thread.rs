use std::{
    sync::mpsc::{self, SyncSender},
    thread::{self, JoinHandle},
    time::Duration,
};

use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    System::Threading::GetCurrentThreadId,
    UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, TranslateMessage, WM_QUIT,
        WM_USER,
    },
};
use winpilot_core::{Context, Error, Result};

use crate::cancel::Cancel;

const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);

pub trait ThreadMessages {
    fn on_message(&mut self, message: &MSG) -> bool;
}

#[derive(Debug)]
pub struct MessageThread {
    thread_id: u32,
    handle: Option<JoinHandle<()>>,
}

type Startup<R> = Result<(u32, R)>;

impl MessageThread {
    pub fn spawn<S, R, F>(name: &str, setup: F) -> Result<(Self, R)>
    where
        S: ThreadMessages,
        R: Send + 'static,
        F: FnOnce() -> Result<(S, R)> + Send + 'static,
    {
        Self::spawn_within(name, STARTUP_TIMEOUT, move |_: &Cancel| setup())
    }

    pub fn spawn_within<S, R, F>(name: &str, startup: Duration, setup: F) -> Result<(Self, R)>
    where
        S: ThreadMessages,
        R: Send + 'static,
        F: FnOnce(&Cancel) -> Result<(S, R)> + Send + 'static,
    {
        let (ready_sender, ready) = mpsc::sync_channel::<Startup<R>>(0);
        let cancel = Cancel::default();
        let thread_cancel = cancel.clone();
        let handle = thread::Builder::new()
            .name(name.to_string())
            .spawn(move || run(setup, &ready_sender, &thread_cancel))
            .context(&format!("starting the {name} thread"))?;
        match ready.recv_timeout(startup) {
            Ok(Ok((thread_id, value))) => Ok((Self { thread_id, handle: Some(handle) }, value)),
            Ok(Err(error)) => {
                let _ = handle.join();
                Err(error)
            }
            Err(_) => {
                cancel.cancel();
                Err(Error::internal(format!("the {name} thread did not start")))
            }
        }
    }

    pub fn post(&self, message: u32) -> bool {
        unsafe { PostThreadMessageW(self.thread_id, message, WPARAM(0), LPARAM(0)) }.is_ok()
    }

    pub fn is_finished(&self) -> bool {
        self.handle.as_ref().is_none_or(JoinHandle::is_finished)
    }

    pub fn thread_id(&self) -> u32 {
        self.thread_id
    }
}

impl Drop for MessageThread {
    fn drop(&mut self) {
        let posted = self.post(WM_QUIT);
        let Some(handle) = self.handle.take() else { return };
        if posted || handle.is_finished() {
            let _ = handle.join();
        }
    }
}

fn run<S, R, F>(setup: F, ready: &SyncSender<Startup<R>>, cancel: &Cancel)
where
    S: ThreadMessages,
    F: FnOnce(&Cancel) -> Result<(S, R)>,
{
    ensure_message_queue();
    match setup(cancel) {
        Ok((mut state, value)) => {
            let thread_id = unsafe { GetCurrentThreadId() };
            if ready.send(Ok((thread_id, value))).is_ok() {
                pump_messages(&mut state);
            }
        }
        Err(error) => {
            let _ = ready.send(Err(error));
        }
    }
}

fn ensure_message_queue() {
    let mut message = MSG::default();
    let _has_message = unsafe { PeekMessageW(&raw mut message, None, WM_USER, WM_USER, PM_NOREMOVE) };
}

fn pump_messages<S: ThreadMessages>(state: &mut S) {
    let mut message = MSG::default();
    loop {
        let status = unsafe { GetMessageW(&raw mut message, None, 0, 0) }.0;
        if status == 0 || status == -1 {
            return;
        }
        if !state.on_message(&message) {
            dispatch(&message);
        }
    }
}

fn dispatch(message: &MSG) {
    let _translated = unsafe { TranslateMessage(message) };
    let _result = unsafe { DispatchMessageW(message) };
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, AtomicU32, Ordering},
        },
        time::Instant,
    };

    use windows::Win32::UI::WindowsAndMessaging::WM_APP;

    use super::*;

    const GAVE_UP: &str = "the caller stopped waiting for this thread to start";

    struct Counter(Arc<AtomicU32>);

    impl ThreadMessages for Counter {
        fn on_message(&mut self, message: &MSG) -> bool {
            if message.message == WM_APP {
                self.0.fetch_add(1, Ordering::SeqCst);
                return true;
            }
            false
        }
    }

    struct Idle(Arc<AtomicBool>);

    impl ThreadMessages for Idle {
        fn on_message(&mut self, _message: &MSG) -> bool {
            false
        }
    }

    impl Drop for Idle {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    fn assert_send_sync<T: Send + Sync>() {}

    fn eventually(done: impl Fn() -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !done() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        done()
    }

    #[test]
    fn message_thread_is_send_and_sync() {
        assert_send_sync::<MessageThread>();
    }

    #[test]
    fn thread_handles_posts_and_stops_on_drop() {
        let count = Arc::new(AtomicU32::new(0));
        let shared = Arc::clone(&count);
        let (thread, greeting) = MessageThread::spawn("test-loop", move || Ok((Counter(shared), 7u8))).unwrap();
        assert_eq!(greeting, 7);
        assert!(thread.post(WM_APP));
        assert!(thread.post(WM_APP));
        drop(thread);
        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn thread_tears_down_its_state_when_dropped() {
        let stopped = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stopped);
        let (thread, ()) = MessageThread::spawn("test-idle", move || Ok((Idle(flag), ()))).unwrap();
        assert!(!stopped.load(Ordering::SeqCst));
        drop(thread);
        assert!(stopped.load(Ordering::SeqCst));
    }

    #[test]
    fn a_thread_that_quits_by_itself_reports_finished() {
        let flag = Arc::new(AtomicBool::new(false));
        let (thread, ()) = MessageThread::spawn("test-quit", move || Ok((Idle(flag), ()))).unwrap();
        assert!(!thread.is_finished());
        assert!(thread.post(WM_QUIT));
        assert!(eventually(|| thread.is_finished()));
    }

    #[test]
    fn a_thread_that_starts_too_late_is_told_to_give_up() {
        let (report, reports) = mpsc::channel();
        let flag = Arc::new(AtomicBool::new(false));
        let torn_down = Arc::clone(&flag);
        let outcome = MessageThread::spawn_within("test-late", Duration::from_millis(50), move |cancel| {
            thread::sleep(Duration::from_millis(250));
            report.send(cancel.check(GAVE_UP).is_err()).unwrap();
            cancel.check(GAVE_UP)?;
            Ok((Idle(torn_down), ()))
        });
        assert!(outcome.is_err());
        assert_eq!(reports.recv_timeout(Duration::from_secs(5)), Ok(true));
        assert!(!flag.load(Ordering::SeqCst));
    }

    #[test]
    fn a_start_the_caller_never_took_tears_the_thread_down() {
        let (ready_sender, ready) = mpsc::sync_channel::<Startup<()>>(0);
        let (set_up, setups) = mpsc::channel();
        let stopped = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stopped);
        let setup = move |_: &Cancel| {
            set_up.send(()).unwrap();
            Ok((Idle(flag), ()))
        };
        thread::spawn(move || run(setup, &ready_sender, &Cancel::default()));
        setups.recv().unwrap();
        thread::sleep(Duration::from_millis(100));
        drop(ready);
        assert!(eventually(|| stopped.load(Ordering::SeqCst)));
    }

    #[test]
    fn setup_failure_is_reported() {
        let outcome = MessageThread::spawn("test-fail", || Err::<(Counter, ()), _>(Error::internal("nope")));
        assert_eq!(outcome.unwrap_err().message, "nope");
    }
}
