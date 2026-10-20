use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use winpilot_core::{Error, Result};

#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn check(&self, message: &str) -> Result<()> {
        if self.0.load(Ordering::SeqCst) { Err(Error::cancelled(message)) } else { Ok(()) }
    }
}

#[cfg(test)]
mod tests {
    use winpilot_core::ErrorCode;

    use super::*;

    #[test]
    fn clones_share_the_flag() {
        let cancel = Cancel::default();
        let clone = cancel.clone();
        assert!(clone.check("stopped").is_ok());
        cancel.cancel();
        let error = clone.check("stopped").unwrap_err();
        assert_eq!((error.code, error.message.as_str()), (ErrorCode::Cancelled, "stopped"));
    }
}
