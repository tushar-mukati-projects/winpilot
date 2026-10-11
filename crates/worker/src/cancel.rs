use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use deckhand_core::{Error, ErrorCode, Result};

#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() { Err(Error::new(ErrorCode::Cancelled, "the request was cancelled")) } else { Ok(()) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_the_flag() {
        let cancel = Cancel::default();
        let clone = cancel.clone();
        assert!(clone.check().is_ok());
        cancel.cancel();
        assert_eq!(clone.check().unwrap_err().code, ErrorCode::Cancelled);
    }
}
