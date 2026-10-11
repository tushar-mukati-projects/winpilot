use std::mem;

use deckhand_core::ElementRef;

use super::{Entry, Registry};
use crate::fingerprint::{self, Fingerprint};

impl<T> Entry<T> {
    fn repeats_any(&self, positional: &[&Fingerprint]) -> bool {
        self.runtime_id.is_empty() && positional.contains(&&self.fingerprint)
    }
}

impl<T: Clone> Registry<T> {
    pub(crate) fn fresh_matches(
        &self,
        stale: ElementRef,
        target: &Fingerprint,
        first_ref_of_rewalk: u32,
    ) -> Vec<ElementRef> {
        let positional = self.positional_refs(stale, first_ref_of_rewalk);
        let mut fresh: Vec<(ElementRef, &Fingerprint)> = self
            .entries
            .iter()
            .filter(|(id, entry)| id.0 >= first_ref_of_rewalk && !entry.repeats_any(&positional))
            .map(|(id, entry)| (*id, &entry.fingerprint))
            .collect();
        fresh.sort_unstable_by_key(|(id, _)| *id);
        fingerprint::top_matches(target, fresh.into_iter())
    }

    pub(crate) fn adopt_replacement(
        &mut self,
        stale: ElementRef,
        replacement: ElementRef,
        first_ref_of_rewalk: u32,
    ) -> bool {
        if replacement.0 < first_ref_of_rewalk || stale.0 >= first_ref_of_rewalk || !self.entries.contains_key(&stale) {
            return false;
        }
        let Some(source) = self.entries.remove(&replacement) else { return false };
        let Entry { live, runtime_id, fingerprint, origin, last_active } = source;
        let Some(target) = self.entries.get_mut(&stale) else { return false };
        let old_runtime_id = mem::replace(&mut target.runtime_id, runtime_id.clone());
        target.live = live;
        target.fingerprint = fingerprint;
        target.origin = origin;
        target.last_active = last_active;
        self.forget_runtime_id(stale, &old_runtime_id);
        if !runtime_id.is_empty() {
            self.by_runtime.insert(runtime_id, stale);
        }
        true
    }

    fn positional_refs(&self, stale: ElementRef, first_ref_of_rewalk: u32) -> Vec<&Fingerprint> {
        self.entries
            .iter()
            .filter(|(id, entry)| id.0 < first_ref_of_rewalk && **id != stale && entry.runtime_id.is_empty())
            .map(|(_, entry)| &entry.fingerprint)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use crate::registry::{
        Missing, Registry,
        tests::{WINDOW, sighting, sighting_at, snapshot},
    };

    #[test]
    fn refs_issued_before_the_rewalk_are_never_candidates() {
        let mut registry = Registry::new(1);
        let now = Instant::now();
        let ids = snapshot(&mut registry, &[1, 2], now);
        let target = registry.resolve(ids[0], now).unwrap().fingerprint;
        let first_ref_of_rewalk = registry.next_ref();
        let generation = registry.generation(WINDOW);
        assert_eq!(registry.record(sighting(20, 2, "Save"), generation, now), ids[1]);
        assert!(registry.fresh_matches(ids[0], &target, first_ref_of_rewalk).is_empty());
        assert!(!registry.adopt_replacement(ids[0], ids[1], first_ref_of_rewalk));
        assert_eq!(registry.resolve(ids[1], now).unwrap().live, 20);
        assert_eq!(registry.revisit(&[42, 2], &21, now), Some(ids[1]));
    }

    #[test]
    fn the_single_closest_fresh_candidate_is_adopted() {
        let mut registry = Registry::new(1);
        let now = Instant::now();
        let generation = registry.begin_snapshot(WINDOW);
        let stale = registry.record(sighting_at(1, 1, "Save", &[0, 2]), generation, now);
        let target = registry.resolve(stale, now).unwrap().fingerprint;
        let first_ref_of_rewalk = registry.next_ref();
        let far = registry.record(sighting_at(2, 2, "Save", &[5]), generation, now);
        let near = registry.record(sighting_at(77, 3, "Save", &[0, 2]), generation, now);
        assert_eq!(registry.fresh_matches(stale, &target, first_ref_of_rewalk), [near]);
        assert!(registry.adopt_replacement(stale, near, first_ref_of_rewalk));
        assert_eq!(registry.resolve(stale, now).unwrap().live, 77);
        assert_eq!(registry.resolve(near, now).err(), Some(Missing::Gone));
        assert!(registry.resolve(far, now).is_ok());
        let again = registry.begin_snapshot(WINDOW);
        assert_eq!(registry.record(sighting_at(78, 3, "Save", &[0, 2]), again, now), stale);
        assert_ne!(registry.record(sighting_at(79, 1, "Save", &[0, 2]), again, now), stale);
    }

    #[test]
    fn equally_close_fresh_candidates_are_all_reported() {
        let mut registry = Registry::new(1);
        let now = Instant::now();
        let stale = snapshot(&mut registry, &[1], now)[0];
        let target = registry.resolve(stale, now).unwrap().fingerprint;
        let first_ref_of_rewalk = registry.next_ref();
        let generation = registry.generation(WINDOW);
        let first = registry.record(sighting(2, 2, "Save"), generation, now);
        let second = registry.record(sighting(3, 3, "Save"), generation, now);
        assert_eq!(registry.fresh_matches(stale, &target, first_ref_of_rewalk), [first, second]);
    }

    #[test]
    fn fresh_elements_that_differ_are_no_candidates() {
        let mut registry = Registry::new(1);
        let now = Instant::now();
        let stale = snapshot(&mut registry, &[1], now)[0];
        let target = registry.resolve(stale, now).unwrap().fingerprint;
        let first_ref_of_rewalk = registry.next_ref();
        let generation = registry.generation(WINDOW);
        registry.record(sighting(2, 2, "Open"), generation, now);
        assert!(registry.fresh_matches(stale, &target, first_ref_of_rewalk).is_empty());
    }

    #[test]
    fn fresh_copies_of_refs_without_runtime_ids_are_no_candidates() {
        let mut registry = Registry::new(1);
        let now = Instant::now();
        let generation = registry.begin_snapshot(WINDOW);
        let stale = registry.record(sighting_at(1, 1, "Save", &[0]), generation, now);
        let mut positional = sighting_at(2, 0, "Save", &[1]);
        positional.runtime_id.clear();
        let existing = registry.record(positional, generation, now);
        let target = registry.resolve(stale, now).unwrap().fingerprint;
        let first_ref_of_rewalk = registry.next_ref();
        let mut again = sighting_at(2, 0, "Save", &[1]);
        again.runtime_id.clear();
        let copy = registry.record(again, generation, now);
        assert_ne!(copy, existing);
        assert!(registry.fresh_matches(stale, &target, first_ref_of_rewalk).is_empty());
    }

    #[test]
    fn a_stale_ref_without_a_runtime_id_can_still_be_found_again() {
        let mut registry = Registry::new(1);
        let now = Instant::now();
        let generation = registry.begin_snapshot(WINDOW);
        let mut original = sighting_at(1, 0, "Save", &[1]);
        original.runtime_id.clear();
        let stale = registry.record(original, generation, now);
        let target = registry.resolve(stale, now).unwrap().fingerprint;
        let first_ref_of_rewalk = registry.next_ref();
        let mut again = sighting_at(2, 0, "Save", &[1]);
        again.runtime_id.clear();
        let fresh = registry.record(again, generation, now);
        assert_eq!(registry.fresh_matches(stale, &target, first_ref_of_rewalk), [fresh]);
    }

    #[test]
    fn adoption_refuses_fresh_stale_refs_and_unknown_replacements() {
        let mut registry = Registry::new(1);
        let now = Instant::now();
        let stale = snapshot(&mut registry, &[1], now)[0];
        let first_ref_of_rewalk = registry.next_ref();
        let generation = registry.generation(WINDOW);
        let fresh = registry.record(sighting(2, 2, "Save"), generation, now);
        let other = registry.record(sighting(3, 3, "Save"), generation, now);
        assert!(!registry.adopt_replacement(fresh, other, first_ref_of_rewalk));
        assert!(!registry.adopt_replacement(stale, deckhand_core::ElementRef(999), first_ref_of_rewalk));
        assert!(registry.resolve(fresh, now).is_ok());
        assert!(registry.resolve(other, now).is_ok());
    }
}
