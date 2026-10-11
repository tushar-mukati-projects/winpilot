use std::marker::PhantomData;

use deckhand_core::{Context, Result};
use windows::Win32::{
    Foundation::RPC_E_CHANGED_MODE,
    System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize},
};

pub(crate) struct ComApartment {
    owns_initialization: bool,
    _bound_to_thread: PhantomData<*const ()>,
}

impl ComApartment {
    pub(crate) fn enter_mta() -> Result<Self> {
        let status = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if status == RPC_E_CHANGED_MODE {
            return Ok(Self { owns_initialization: false, _bound_to_thread: PhantomData });
        }
        status.ok().context("initialising COM")?;
        Ok(Self { owns_initialization: true, _bound_to_thread: PhantomData })
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
        let outer = ComApartment::enter_mta().unwrap();
        let inner = ComApartment::enter_mta().unwrap();
        drop(inner);
        drop(outer);
    }
}
