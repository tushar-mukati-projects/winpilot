use std::time::Duration;

use deckhand_core::Result;

pub struct NamedMutex {
    _private: (),
}

pub struct NamedMutexGuard<'a> {
    _mutex: &'a NamedMutex,
}

impl NamedMutex {
    pub fn create(name: &str) -> Result<Self> {
        todo!("{name}")
    }

    pub fn lock(&self, timeout: Duration) -> Result<Option<NamedMutexGuard<'_>>> {
        todo!("{timeout:?}")
    }
}

pub struct NamedEvent {
    _private: (),
}

impl NamedEvent {
    pub fn create(name: &str) -> Result<Self> {
        todo!("{name}")
    }

    pub fn set(&self) -> Result<()> {
        todo!()
    }

    pub fn reset(&self) -> Result<()> {
        todo!()
    }

    pub fn is_set(&self) -> bool {
        todo!()
    }
}
