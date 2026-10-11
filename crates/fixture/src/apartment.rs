use std::marker::PhantomData;

use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx, CoUninitialize};
use windows::core::Result;

pub struct StaApartment {
    _bound_to_thread: PhantomData<*const ()>,
}

impl StaApartment {
    pub fn enter() -> Result<Self> {
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) }.ok()?;
        Ok(Self { _bound_to_thread: PhantomData })
    }
}

impl Drop for StaApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}
