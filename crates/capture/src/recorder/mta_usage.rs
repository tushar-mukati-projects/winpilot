use std::sync::OnceLock;

use deckhand_core::{Error, Result};
use windows::Win32::System::Com::CoIncrementMTAUsage;

static PROCESS_MTA: OnceLock<std::result::Result<usize, String>> = OnceLock::new();

pub(crate) fn keep_mta_for_process() -> Result<()> {
    match PROCESS_MTA.get_or_init(increment_mta_usage) {
        Ok(_) => Ok(()),
        Err(message) => Err(Error::unsupported(message.clone())),
    }
}

fn increment_mta_usage() -> std::result::Result<usize, String> {
    unsafe { CoIncrementMTAUsage() }
        .map(|cookie| cookie.0 as usize)
        .map_err(|error| format!("COM's multithreaded apartment could not be kept alive: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_process_mta_is_kept_once_and_reused() {
        keep_mta_for_process().unwrap();
        keep_mta_for_process().unwrap();
        assert!(PROCESS_MTA.get().is_some_and(std::result::Result::is_ok));
    }
}
