use std::marker::PhantomData;

use deckhand_core::{Context, Result};
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};

pub(crate) struct ComApartment {
    owns_initialization: bool,
    _thread_bound: PhantomData<*const ()>,
}

impl ComApartment {
    pub(crate) fn enter_mta_strict() -> Result<Self> {
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        result.ok().context("initialising COM (MTA)")?;
        Ok(Self::new(true))
    }

    pub(crate) fn enter_any() -> Result<Self> {
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result == RPC_E_CHANGED_MODE {
            return Ok(Self::new(false));
        }
        result.ok().context("initialising COM")?;
        Ok(Self::new(true))
    }

    const fn new(owns_initialization: bool) -> Self {
        Self { owns_initialization, _thread_bound: PhantomData }
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.owns_initialization {
            unsafe { CoUninitialize() };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_entries_balance() {
        let outer = ComApartment::enter_mta_strict().unwrap();
        let inner = ComApartment::enter_any().unwrap();
        assert!(inner.owns_initialization);
        drop(inner);
        drop(outer);
    }
}
