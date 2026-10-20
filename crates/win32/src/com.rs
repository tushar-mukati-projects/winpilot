use std::marker::PhantomData;

use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use winpilot_core::{Context, Result};

pub struct ComApartment {
    owns_initialization: bool,
    _bound_to_thread: PhantomData<*const ()>,
}

impl ComApartment {
    pub fn enter_mta_strict() -> Result<Self> {
        let status = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        status.ok().context("initialising COM (MTA)")?;
        Ok(Self::new(true))
    }

    pub fn enter_any() -> Result<Self> {
        let status = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if status == RPC_E_CHANGED_MODE {
            return Ok(Self::new(false));
        }
        status.ok().context("initialising COM")?;
        Ok(Self::new(true))
    }

    const fn new(owns_initialization: bool) -> Self {
        Self { owns_initialization, _bound_to_thread: PhantomData }
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
        assert!(outer.owns_initialization && inner.owns_initialization);
        drop(inner);
        drop(outer);
    }

    #[test]
    fn tolerant_entries_nest() {
        let outer = ComApartment::enter_any().unwrap();
        let inner = ComApartment::enter_any().unwrap();
        drop(inner);
        drop(outer);
    }
}
