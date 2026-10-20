use std::marker::PhantomData;

use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::Threading::{
    GetCurrentThread, GetThreadPriority, SetThreadPriority, THREAD_PRIORITY, THREAD_PRIORITY_BELOW_NORMAL,
};
use winpilot_core::{Context, Error, Result};

const PRIORITY_ERROR_RETURN: i32 = 0x7FFF_FFFF;

pub(crate) struct BelowNormalPriority {
    previous: THREAD_PRIORITY,
    _thread_bound: PhantomData<*const ()>,
}

impl BelowNormalPriority {
    pub(crate) fn enter() -> Result<Self> {
        let previous = current_thread_priority()?;
        lower_current_thread()?;
        Ok(Self { previous, _thread_bound: PhantomData })
    }
}

impl Drop for BelowNormalPriority {
    fn drop(&mut self) {
        let _ = set_current_thread_priority(self.previous);
    }
}

pub(crate) fn lower_current_thread() -> Result<()> {
    set_current_thread_priority(THREAD_PRIORITY_BELOW_NORMAL)
}

fn current_thread_priority() -> Result<THREAD_PRIORITY> {
    let priority = unsafe { GetThreadPriority(current_thread()) };
    if priority == PRIORITY_ERROR_RETURN {
        return Err(Error::internal(format!("reading the thread priority: {}", windows::core::Error::from_thread())));
    }
    Ok(THREAD_PRIORITY(priority))
}

fn set_current_thread_priority(priority: THREAD_PRIORITY) -> Result<()> {
    unsafe { SetThreadPriority(current_thread(), priority) }.context("setting the thread priority")
}

fn current_thread() -> HANDLE {
    unsafe { GetCurrentThread() }
}

pub(crate) fn run_below_normal<T: Send>(threads: usize, work: impl FnOnce() -> T + Send) -> Result<T> {
    let _caller = BelowNormalPriority::enter()?;
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .thread_name(|index| format!("winpilot-speech-{index}"))
        .start_handler(|_| {
            let _ = lower_current_thread();
        })
        .build()
        .context("starting the speech threads")?;
    Ok(pool.install(work))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_is_lowered_and_restored() {
        std::thread::spawn(|| {
            let before = current_thread_priority().unwrap();
            {
                let _guard = BelowNormalPriority::enter().unwrap();
                assert_eq!(current_thread_priority().unwrap(), THREAD_PRIORITY_BELOW_NORMAL);
            }
            assert_eq!(current_thread_priority().unwrap(), before);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn pool_threads_run_below_normal() {
        let (priority, threads) =
            run_below_normal(2, || (current_thread_priority().unwrap(), rayon::current_num_threads())).unwrap();
        assert_eq!(priority, THREAD_PRIORITY_BELOW_NORMAL);
        assert_eq!(threads, 2);
    }
}
