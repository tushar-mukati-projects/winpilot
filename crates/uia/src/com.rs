use std::marker::PhantomData;

use deckhand_core::{Context, Result};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};

pub(crate) struct ComApartment {
    _bound_to_thread: PhantomData<*const ()>,
}

impl ComApartment {
    pub(crate) fn enter_mta_strict() -> Result<Self> {
        let status = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        status.ok().context("initialising COM (multithreaded) on the UI Automation thread")?;
        Ok(Self { _bound_to_thread: PhantomData })
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}
