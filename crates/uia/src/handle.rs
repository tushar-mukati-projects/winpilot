use std::{
    sync::{
        Arc,
        mpsc::{self, Receiver, Sender, SyncSender},
    },
    thread::{self, JoinHandle, ThreadId},
};

use windows::Win32::UI::HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetThreadDpiAwarenessContext};
use winpilot_core::{Context, Error, Result};
use winpilot_win32::ComApartment;

use crate::{automation::Uia, options::UiaOptions};

type Task = Box<dyn FnOnce(&mut Uia) + Send>;

#[derive(Clone)]
pub struct UiaHandle {
    shared: Arc<Shared>,
}

struct Shared {
    sender: Option<Sender<Task>>,
    thread: Option<JoinHandle<()>>,
    thread_id: ThreadId,
}

struct Session {
    uia: Uia,
    _apartment: ComApartment,
}

impl UiaHandle {
    pub fn spawn(options: UiaOptions) -> Result<Self> {
        let (sender, tasks) = mpsc::channel::<Task>();
        let (ready_sender, ready) = mpsc::sync_channel::<Result<()>>(1);
        let thread = thread::Builder::new()
            .name("winpilot-uia".to_string())
            .spawn(move || serve(options, &tasks, &ready_sender))
            .context("starting the UI Automation thread")?;
        let shared = Shared { sender: Some(sender), thread_id: thread.thread().id(), thread: Some(thread) };
        ready.recv().map_err(|_| stopped())??;
        Ok(Self { shared: Arc::new(shared) })
    }

    pub fn run<R, F>(&self, task: F) -> Result<R>
    where
        F: FnOnce(&mut Uia) -> Result<R> + Send + 'static,
        R: Send + 'static,
    {
        if thread::current().id() == self.shared.thread_id {
            return Err(Error::internal("UiaHandle::run was called from the UI Automation thread itself"));
        }
        let sender = self.shared.sender.as_ref().ok_or_else(stopped)?;
        let (reply_sender, reply) = mpsc::sync_channel(1);
        let job: Task = Box::new(move |uia: &mut Uia| {
            let _ = reply_sender.send(task(uia));
        });
        sender.send(job).map_err(|_| stopped())?;
        reply.recv().map_err(|_| stopped())?
    }
}

impl Drop for Shared {
    fn drop(&mut self) {
        drop(self.sender.take());
        let Some(thread) = self.thread.take() else { return };
        if thread.thread().id() != thread::current().id() {
            let _ = thread.join();
        }
    }
}

impl Session {
    fn open(options: UiaOptions) -> Result<Self> {
        let apartment = ComApartment::enter_mta_strict()?;
        use_physical_pixels()?;
        let uia = Uia::create(options)?;
        Ok(Self { uia, _apartment: apartment })
    }
}

fn stopped() -> Error {
    Error::internal("UI Automation thread stopped")
}

fn serve(options: UiaOptions, tasks: &Receiver<Task>, ready: &SyncSender<Result<()>>) {
    let mut session = match Session::open(options) {
        Ok(session) => session,
        Err(error) => {
            let _ = ready.send(Err(error));
            return;
        }
    };
    if ready.send(Ok(())).is_err() {
        return;
    }
    while let Ok(task) = tasks.recv() {
        task(&mut session.uia);
    }
}

fn use_physical_pixels() -> Result<()> {
    let previous = unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
    if previous.0.is_null() {
        return Err(Error::internal("making the UI Automation thread per-monitor DPI aware failed"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use winpilot_core::ErrorCode;

    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    fn assert_clone<T: Clone>() {}

    #[test]
    fn handle_is_shareable_across_threads() {
        assert_send_sync::<UiaHandle>();
        assert_clone::<UiaHandle>();
    }

    #[test]
    fn run_returns_the_task_result() {
        let handle = UiaHandle::spawn(UiaOptions::default()).unwrap();
        assert_eq!(handle.run(|_| Ok(7)).unwrap(), 7);
        let copy = handle.clone();
        assert_eq!(copy.run(|_| Ok("again")).unwrap(), "again");
    }

    #[test]
    fn task_errors_come_back_unchanged() {
        let handle = UiaHandle::spawn(UiaOptions::default()).unwrap();
        let error = handle.run(|_| -> Result<()> { Err(Error::not_found("nothing")) }).unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
        assert_eq!(handle.run(|_| Ok(1)).unwrap(), 1);
    }

    #[test]
    fn a_dead_thread_reports_stopped() {
        let handle = UiaHandle::spawn(UiaOptions::default()).unwrap();
        let crashed = handle.run(|_| -> Result<()> { panic!("task panicked on purpose") });
        assert_eq!(crashed.unwrap_err().message, "UI Automation thread stopped");
        let after = handle.run(|_| Ok(())).unwrap_err();
        assert_eq!(after.message, "UI Automation thread stopped");
    }

    #[test]
    fn handles_work_from_other_threads() {
        let handle = UiaHandle::spawn(UiaOptions::default()).unwrap();
        let workers: Vec<_> = (0..4)
            .map(|index| {
                let handle = handle.clone();
                thread::spawn(move || handle.run(move |_| Ok(index * 2)).unwrap())
            })
            .collect();
        let results: Vec<i32> = workers.into_iter().map(|worker| worker.join().unwrap()).collect();
        assert_eq!(results, [0, 2, 4, 6]);
    }
}
