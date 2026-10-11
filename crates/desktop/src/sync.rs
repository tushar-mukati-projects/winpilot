use std::{marker::PhantomData, os::windows::io::OwnedHandle, time::Duration};

use deckhand_core::{Context, Error, Result};
use windows::{
    Win32::{
        Foundation::{WAIT_ABANDONED, WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{CreateEventW, CreateMutexW, ReleaseMutex, ResetEvent, SetEvent, WaitForSingleObject},
    },
    core::{HSTRING, PCWSTR},
};

use crate::{
    handle::{adopt, raw},
    timeouts::finite_millis,
};

pub struct NamedMutex {
    handle: OwnedHandle,
}

pub struct NamedMutexGuard<'a> {
    mutex: &'a NamedMutex,
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
            WAIT_OBJECT_0 | WAIT_ABANDONED => {
                Ok(Some(NamedMutexGuard { mutex: self, _owned_by_this_thread: PhantomData }))
            }
            WAIT_TIMEOUT => Ok(None),
            _ => Err(windows::core::Error::from_thread()).context("waiting for a mutex"),
        }
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
        format!("Local\\deckhand-test-{kind}-{}-{serial}", std::process::id())
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
