use std::marker::PhantomData;

use windows::Win32::Media::MediaFoundation::{MF_VERSION, MFSTARTUP_NOSOCKET, MFShutdown, MFStartup};
use winpilot_core::{Error, Result};

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
