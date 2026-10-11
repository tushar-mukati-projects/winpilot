use std::time::Instant;

use crate::{automation::Uia, win};

impl Uia {
    pub(crate) fn evict_stale_refs(&mut self) {
        self.registry.evict(Instant::now(), win::window_exists);
        self.warmed.prune(win::window_exists);
    }
}
