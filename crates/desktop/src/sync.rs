use std::{marker::PhantomData, os::windows::io::OwnedHandle, time::Duration};

use windows::{
    Win32::{
        Foundation::{WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{CreateEventW, CreateMutexW, ReleaseMutex, ResetEvent, SetEvent, WaitForSingleObject},
    },
    core::{HSTRING, PCWSTR},
};
use winpilot_core::{Context, Error, Result};

use crate::{
    handle::{adopt, raw},
    timeouts::finite_millis,
};

pub struct NamedMutex {
    handle: OwnedHandle,
}

pub struct NamedMutexGuard<'a> {
    mutex: &'a NamedMutex,
    abandoned: bool,
    _owned_by_this_thread: PhantomData<*const ()>,
}

impl NamedMutex {
    pub fn create(name: &str) -> Result<Self> {
        let name_text = object_name(name)?;
        let created = unsafe { CreateMutexW(None, false, PCWSTR(name_text.as_ptr())) }
            .context(&format!("creating mutex {name}"))?;
        let handle = adopt(created).context("mutex handle")?;
        Ok(Self { handle })
    }

    pub fn lock(&self, timeout: Duration) -> Result<Option<NamedMutexGuard<'_>>> {
        let outcome = unsafe { WaitForSingleObject(raw(&self.handle), finite_millis(timeout)) };
        match outcome {
            WAIT_OBJECT_0 => Ok(Some(self.guard(false))),
            WAIT_ABANDONED => Ok(Some(self.guard(true))),
            WAIT_TIMEOUT => Ok(None),
            _ => Err(windows::core::Error::from_thread()).context("waiting for a mutex"),
        }
    }

    pub fn try_lock_abandoned(&self) -> Result<Option<NamedMutexGuard<'_>>> {
        Ok(self.lock(Duration::ZERO)?.filter(NamedMutexGuard::was_abandoned))
    }

    fn guard(&self, abandoned: bool) -> NamedMutexGuard<'_> {
        NamedMutexGuard { mutex: self, abandoned, _owned_by_this_thread: PhantomData }
    }
}

impl NamedMutexGuard<'_> {
    pub fn was_abandoned(&self) -> bool {
        self.abandoned
    }
}

impl Drop for NamedMutexGuard<'_> {
    fn drop(&mut self) {
        let _ = unsafe { ReleaseMutex(raw(&self.mutex.handle)) };
    }
}

pub struct NamedEvent {
    handle: OwnedHandle,
}

impl NamedEvent {
    pub fn create(name: &str) -> Result<Self> {
        let name_text = object_name(name)?;
        let created = unsafe { CreateEventW(None, true, false, PCWSTR(name_text.as_ptr())) }
            .context(&format!("creating event {name}"))?;
        let handle = adopt(created).context("event handle")?;
        Ok(Self { handle })
    }

    pub fn set(&self) -> Result<()> {
        unsafe { SetEvent(raw(&self.handle)) }.context("setting an event")
    }

    pub fn reset(&self) -> Result<()> {
        unsafe { ResetEvent(raw(&self.handle)) }.context("resetting an event")
    }

    pub fn is_set(&self) -> bool {
        let outcome = unsafe { WaitForSingleObject(raw(&self.handle), 0) };
        outcome == WAIT_OBJECT_0
    }
}

fn object_name(name: &str) -> Result<HSTRING> {
    if name.is_empty() || name.contains('\0') {
        return Err(Error::invalid("a named sync object needs a non-empty name without NUL"));
    }
    Ok(HSTRING::from(name))
}

#[cfg(test)]
mod tests {
    use std::{
        sync::atomic::{AtomicU32, Ordering},
        sync::mpsc,
        thread,
    };

    use super::*;

    static NEXT_NAME: AtomicU32 = AtomicU32::new(0);

    fn unique_name(kind: &str) -> String {
        let serial = NEXT_NAME.fetch_add(1, Ordering::Relaxed);
        format!("Local\\winpilot-test-{kind}-{}-{serial}", std::process::id())
    }

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn sync_objects_are_send_and_sync() {
        assert_send_sync::<NamedMutex>();
        assert_send_sync::<NamedEvent>();
    }

    #[test]
    fn second_holder_waits_for_release() {
        let name = unique_name("mutex");
        let mutex = NamedMutex::create(&name).unwrap();
        let guard = mutex.lock(Duration::from_millis(100)).unwrap().unwrap();
        assert!(!guard.was_abandoned());
        let (to_main, from_contender) = mpsc::channel();
        let (to_contender, from_main) = mpsc::channel::<()>();
        let contender = thread::spawn(move || {
            let same = NamedMutex::create(&name).unwrap();
            to_main.send(same.lock(Duration::from_millis(50)).unwrap().is_some()).unwrap();
            from_main.recv().unwrap();
            to_main.send(same.lock(Duration::from_secs(2)).unwrap().is_some()).unwrap();
        });
        assert!(!from_contender.recv().unwrap(), "contender got the mutex while it was held");
        drop(guard);
        to_contender.send(()).unwrap();
        assert!(from_contender.recv().unwrap(), "contender never got the released mutex");
        contender.join().unwrap();
    }

    #[test]
    fn a_holder_that_dies_leaves_the_mutex_abandoned() {
        let name = unique_name("abandoned");
        let mutex = NamedMutex::create(&name).unwrap();
        let holder = thread::spawn(move || {
            let same = NamedMutex::create(&name).unwrap();
            std::mem::forget(same.lock(Duration::from_millis(100)).unwrap().unwrap());
        });
        holder.join().unwrap();
        let guard = mutex.lock(Duration::from_secs(2)).unwrap().unwrap();
        assert!(guard.was_abandoned());
        drop(guard);
        let again = mutex.lock(Duration::from_millis(100)).unwrap().unwrap();
        assert!(!again.was_abandoned());
    }

    #[test]
    fn only_an_abandoned_mutex_is_claimed_for_recovery() {
        let name = unique_name("recover");
        let mutex = NamedMutex::create(&name).unwrap();
        assert!(mutex.try_lock_abandoned().unwrap().is_none());
        assert!(mutex.lock(Duration::from_millis(100)).unwrap().is_some_and(|guard| !guard.was_abandoned()));
        let holder = thread::spawn(move || {
            let same = NamedMutex::create(&name).unwrap();
            std::mem::forget(same.lock(Duration::from_millis(100)).unwrap().unwrap());
        });
        holder.join().unwrap();
        let recovering = mutex.try_lock_abandoned().unwrap();
        assert!(recovering.as_ref().is_some_and(NamedMutexGuard::was_abandoned));
        drop(recovering);
        assert!(mutex.try_lock_abandoned().unwrap().is_none());
    }

    #[test]
    fn a_live_holder_is_never_claimed_for_recovery() {
        let name = unique_name("live");
        let mutex = NamedMutex::create(&name).unwrap();
        let _held = mutex.lock(Duration::from_millis(100)).unwrap().unwrap();
        let contender = thread::spawn(move || {
            let same = NamedMutex::create(&name).unwrap();
            same.try_lock_abandoned().unwrap().is_none()
        });
        assert!(contender.join().unwrap());
    }

    #[test]
    fn event_set_reset_and_query() {
        let event = NamedEvent::create(&unique_name("event")).unwrap();
        assert!(!event.is_set());
        event.set().unwrap();
        assert!(event.is_set());
        assert!(event.is_set());
        event.reset().unwrap();
        assert!(!event.is_set());
    }

    #[test]
    fn empty_names_are_rejected() {
        assert!(NamedMutex::create("").is_err());
        assert!(NamedEvent::create("bad\0name").is_err());
    }
}
