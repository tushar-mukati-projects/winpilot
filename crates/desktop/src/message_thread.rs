use std::{
    sync::mpsc::{self, Sender},
    thread::{self, JoinHandle},
    time::Duration,
};

use deckhand_core::{Context, Error, Result};
use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    System::Threading::GetCurrentThreadId,
    UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, TranslateMessage, WM_QUIT,
        WM_USER,
    },
};

const STARTUP_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) trait ThreadMessages {
    fn on_message(&mut self, message: &MSG) -> bool;
}

pub(crate) struct MessageThread {
    thread_id: u32,
    handle: Option<JoinHandle<()>>,
}

type Startup<R> = Result<(u32, R)>;

impl MessageThread {
    pub(crate) fn spawn<S, R, F>(name: &str, setup: F) -> Result<(Self, R)>
    where
        S: ThreadMessages,
        R: Send + 'static,
        F: FnOnce() -> Result<(S, R)> + Send + 'static,
    {
        let (ready_sender, ready) = mpsc::channel::<Startup<R>>();
        let handle = thread::Builder::new()
            .name(name.to_string())
            .spawn(move || run(setup, &ready_sender))
            .context(&format!("starting the {name} thread"))?;
        match ready.recv_timeout(STARTUP_TIMEOUT) {
            Ok(Ok((thread_id, value))) => Ok((Self { thread_id, handle: Some(handle) }, value)),
            Ok(Err(error)) => {
                let _ = handle.join();
                Err(error)
            }
            Err(_) => Err(Error::internal(format!("the {name} thread did not start"))),
        }
    }

    pub(crate) fn post(&self, message: u32) -> bool {
        unsafe { PostThreadMessageW(self.thread_id, message, WPARAM(0), LPARAM(0)) }.is_ok()
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

fn run<S, R, F>(setup: F, ready: &Sender<Startup<R>>)
where
    S: ThreadMessages,
    F: FnOnce() -> Result<(S, R)>,
{
    ensure_message_queue();
    match setup() {
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
    use std::sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    };

    use windows::Win32::UI::WindowsAndMessaging::WM_APP;

    use super::*;

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

    fn assert_send_sync<T: Send + Sync>() {}

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
    fn setup_failure_is_reported() {
        let outcome = MessageThread::spawn("test-fail", || Err::<(Counter, ()), _>(Error::internal("nope")));
        assert!(outcome.is_err());
    }
}
