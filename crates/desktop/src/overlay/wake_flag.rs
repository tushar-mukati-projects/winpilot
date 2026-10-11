use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Clone, Default)]
pub(crate) struct WakeFlag(Arc<AtomicBool>);

impl WakeFlag {
    pub(crate) fn claim(&self) -> bool {
        !self.0.swap(true, Ordering::AcqRel)
    }

    pub(crate) fn give_back(&self) {
        self.0.store(false, Ordering::Release);
    }

    pub(crate) fn take(&self) {
        self.0.swap(false, Ordering::AcqRel);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_first_sender_posts_until_the_thread_takes_the_wake() {
        let sender = WakeFlag::default();
        let thread = sender.clone();
        assert!(sender.claim());
        assert!(!sender.claim());
        thread.take();
        assert!(sender.claim());
    }

    #[test]
    fn a_failed_post_lets_the_next_sender_try_again() {
        let flag = WakeFlag::default();
        assert!(flag.claim());
        flag.give_back();
        assert!(flag.claim());
    }
}
