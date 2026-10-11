use std::{
    sync::mpsc::{SyncSender, sync_channel},
    thread::{self, JoinHandle},
};

use deckhand_core::{Context, Error, Result};
use windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    System::Threading::GetCurrentThreadId,
    UI::WindowsAndMessaging::{GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, WM_QUIT, WM_USER},
};

pub trait MessageLoop {
    fn on_message(&mut self, message: &MSG);
}

#[derive(Debug)]
pub struct MessageThread {
    thread_id: u32,
    handle: Option<JoinHandle<()>>,
}

impl MessageThread {
    pub fn spawn<S, L>(name: &str, setup: S) -> Result<Self>
    where
        S: FnOnce() -> Result<L> + Send + 'static,
        L: MessageLoop,
    {
        let (ready_sender, ready) = sync_channel(1);
        let handle = thread::Builder::new()
            .name(name.to_string())
            .spawn(move || run_thread(setup, &ready_sender))
            .context("starting a message thread")?;
        let started = ready.recv().unwrap_or_else(|_| Err(Error::internal(format!("{name} stopped while starting"))));
        match started {
            Ok(thread_id) => Ok(Self { thread_id, handle: Some(handle) }),
            Err(error) => {
                let _ = handle.join();
                Err(error)
            }
        }
    }
}

impl Drop for MessageThread {
    fn drop(&mut self) {
        let Some(handle) = self.handle.take() else {
            return;
        };
        if post_quit(self.thread_id) || handle.is_finished() {
            let _ = handle.join();
        }
    }
}

fn run_thread<S, L>(setup: S, ready: &SyncSender<Result<u32>>)
where
    S: FnOnce() -> Result<L>,
    L: MessageLoop,
{
    create_message_queue();
    let mut state = match setup() {
        Ok(state) => state,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    if ready.send(Ok(current_thread_id())).is_ok() {
        pump_messages(&mut state);
    }
}

fn create_message_queue() {
    let mut message = MSG::default();
    let _ = unsafe { PeekMessageW(&raw mut message, None, WM_USER, WM_USER, PM_NOREMOVE) };
}

fn pump_messages(state: &mut impl MessageLoop) {
    let mut message = MSG::default();
    while unsafe { GetMessageW(&raw mut message, None, 0, 0) }.0 > 0 {
        state.on_message(&message);
    }
}

fn post_quit(thread_id: u32) -> bool {
    unsafe { PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) }.is_ok()
}

fn current_thread_id() -> u32 {
    unsafe { GetCurrentThreadId() }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use super::*;

    struct Idle(Arc<AtomicBool>);

    impl MessageLoop for Idle {
        fn on_message(&mut self, _message: &MSG) {}
    }

    impl Drop for Idle {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }

    #[test]
    fn thread_stops_and_tears_down_when_dropped() {
        let stopped = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&stopped);
        let thread = MessageThread::spawn("deckhand-test-loop", move || Ok(Idle(flag))).unwrap();
        assert!(!stopped.load(Ordering::SeqCst));
        drop(thread);
        assert!(stopped.load(Ordering::SeqCst));
    }

    #[test]
    fn setup_failure_is_returned_from_spawn() {
        let error =
            MessageThread::spawn("deckhand-test-fail", || -> Result<Idle> { Err(Error::internal("no")) }).unwrap_err();
        assert_eq!(error.message, "no");
    }
}
