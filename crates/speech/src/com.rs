use std::marker::PhantomData;

use deckhand_core::{Context, Error, Result};
use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
use windows::Win32::Media::MediaFoundation::{MF_VERSION, MFSTARTUP_NOSOCKET, MFShutdown, MFStartup};
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};

pub(crate) struct ComApartment {
    owns_initialization: bool,
    _thread_bound: PhantomData<*const ()>,
}

impl ComApartment {
    pub(crate) fn enter_mta() -> Result<Self> {
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result == RPC_E_CHANGED_MODE {
            return Ok(Self { owns_initialization: false, _thread_bound: PhantomData });
        }
        result.ok().context("initialising COM (MTA)")?;
        Ok(Self { owns_initialization: true, _thread_bound: PhantomData })
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.owns_initialization {
            unsafe { CoUninitialize() };
        }
    }
}

pub(crate) struct MediaFoundation {
    _thread_bound: PhantomData<*const ()>,
}

impl MediaFoundation {
    pub(crate) fn start() -> Result<Self> {
        unsafe { MFStartup(MF_VERSION, MFSTARTUP_NOSOCKET) }
            .map_err(|error| Error::unsupported(format!("Media Foundation is not available: {error}")))?;
        Ok(Self { _thread_bound: PhantomData })
    }
}

impl Drop for MediaFoundation {
    fn drop(&mut self) {
        let _ = unsafe { MFShutdown() };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apartment_and_media_foundation_nest() {
        std::thread::spawn(|| {
            let outer = ComApartment::enter_mta().unwrap();
            let inner = ComApartment::enter_mta().unwrap();
            let media = MediaFoundation::start().unwrap();
            drop(media);
            drop(inner);
            drop(outer);
        })
        .join()
        .unwrap();
    }
}
