use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use deckhand_core::{Error, ErrorCode, Result};

#[derive(Debug, Clone, Default)]
pub(crate) struct StartupCancel(Arc<AtomicBool>);

impl StartupCancel {
    pub(crate) fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    pub(crate) fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            return Err(Error::new(ErrorCode::Cancelled, "the caller stopped waiting for this thread to start"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cancel_is_seen_by_every_clone() {
        let caller = StartupCancel::default();
        let thread = caller.clone();
        assert!(thread.check().is_ok());
        caller.cancel();
        assert!(thread.is_cancelled());
        assert_eq!(thread.check().unwrap_err().code, ErrorCode::Cancelled);
    }
}
