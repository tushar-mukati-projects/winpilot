use deckhand_core::{Error, Result};
use parking_lot::Mutex;
use windows::Win32::System::Com::CoIncrementMTAUsage;

static PROCESS_MTA_ANCHOR: Mutex<Option<ProcessMtaAnchor>> = Mutex::new(None);

struct ProcessMtaAnchor {
    _never_released_cookie: usize,
}

pub(crate) fn anchor_mta_for_process_lifetime() -> Result<()> {
    let mut anchor = PROCESS_MTA_ANCHOR.lock();
    if anchor.is_none() {
        *anchor = Some(ProcessMtaAnchor::acquire()?);
    }
    Ok(())
}

impl ProcessMtaAnchor {
    fn acquire() -> Result<Self> {
        let cookie = unsafe { CoIncrementMTAUsage() }.map_err(|error| {
            Error::unsupported(format!("COM's multithreaded apartment could not be kept alive: {error}"))
        })?;
        Ok(Self { _never_released_cookie: cookie.0 as usize })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_process_mta_is_anchored_once_and_reused() {
        anchor_mta_for_process_lifetime().unwrap();
        anchor_mta_for_process_lifetime().unwrap();
        assert!(PROCESS_MTA_ANCHOR.lock().is_some());
    }
}
