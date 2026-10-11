use std::time::Instant;

use deckhand_core::{Element, ElementRef, ErrorCode, Result};

use crate::{
    automation::Uia,
    fingerprint::Fingerprint,
    lookup::{missing_ref, stale_ref},
    options::SnapshotOptions,
    resolved::Resolved,
    snapshot::Walk,
    win,
};

impl Uia {
    pub(crate) fn refind(&mut self, id: ElementRef, fingerprint: &Fingerprint) -> Result<Resolved> {
        if !win::window_exists(fingerprint.window) {
            return Err(stale_ref(id));
        }
        let issued_before = self.registry.watermark();
        let walk = self.rewalk(id, fingerprint)?;
        let replacement = self.replacement_for(id, fingerprint, issued_before).ok_or_else(|| stale_ref(id))?;
        let info = walk
            .elements
            .into_iter()
            .find(|element| element.id == replacement)
            .map(|element| Element { id, ..element })
            .ok_or_else(|| stale_ref(id))?;
        let known = self.registry.resolve(id, Instant::now()).map_err(|missing| missing_ref(id, missing))?;
        self.registry.evict(Instant::now());
        Ok(Resolved { element: known.live, info })
    }

    fn rewalk(&mut self, id: ElementRef, fingerprint: &Fingerprint) -> Result<Walk> {
        let options = SnapshotOptions { include_offscreen: true, ..SnapshotOptions::default() };
        match self.walk_window(fingerprint.window, &options, Instant::now()) {
            Err(error) if error.code == ErrorCode::NotFound => Err(stale_ref(id)),
            walked => walked,
        }
    }

    fn replacement_for(&mut self, id: ElementRef, fingerprint: &Fingerprint, issued_before: u32) -> Option<ElementRef> {
        let generation = self.registry.generation(fingerprint.window);
        if self.registry.seen_in(id, generation) {
            return Some(id);
        }
        let found = self.registry.best_match(fingerprint, generation, id)?;
        self.registry.repoint(id, found, issued_before).then_some(found)
    }
}
