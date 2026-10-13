mod eviction;
mod replacement;

use std::{collections::HashMap, mem, time::Instant};

use winpilot_core::{ElementRef, WindowRef};

use crate::fingerprint::Fingerprint;

pub(crate) struct Sighting<T> {
    pub(crate) live: T,
    pub(crate) runtime_id: Vec<i32>,
    pub(crate) fingerprint: Fingerprint,
}

pub(crate) struct Known<T> {
    pub(crate) live: T,
    pub(crate) fingerprint: Fingerprint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Missing {
    NeverIssued,
    FromPreviousWorker,
    Gone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Origin {
    Snapshot(u64),
    Lookup,
}

struct Entry<T> {
    live: T,
    runtime_id: Vec<i32>,
    fingerprint: Fingerprint,
    origin: Origin,
    last_active: Instant,
}

pub(crate) struct Registry<T> {
    first_ref: u32,
    next_ref: u32,
    entries: HashMap<ElementRef, Entry<T>>,
    by_runtime: HashMap<Vec<i32>, ElementRef>,
    generations: HashMap<WindowRef, u64>,
}

impl<T: Clone> Registry<T> {
    pub(crate) fn new(first_ref: u32) -> Self {
        let first_ref = first_ref.max(1);
        Self {
            first_ref,
            next_ref: first_ref,
            entries: HashMap::new(),
            by_runtime: HashMap::new(),
            generations: HashMap::new(),
        }
    }

    pub(crate) fn begin_snapshot(&mut self, window: WindowRef) -> u64 {
        let generation = self.generations.entry(window).or_insert(0);
        *generation += 1;
        *generation
    }

    pub(crate) fn generation(&self, window: WindowRef) -> u64 {
        self.generations.get(&window).copied().unwrap_or(0)
    }

    pub(crate) fn next_ref(&self) -> u32 {
        self.next_ref
    }

    pub(crate) fn record(&mut self, sighting: Sighting<T>, generation: u64, now: Instant) -> ElementRef {
        let origin = Origin::Snapshot(generation);
        let Some(id) = self.id_for(&sighting.runtime_id) else {
            return self.insert(sighting, origin, now);
        };
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.live = sighting.live;
            entry.fingerprint = sighting.fingerprint;
            entry.origin = origin;
            entry.last_active = now;
        }
        id
    }

    pub(crate) fn revisit(&mut self, runtime_id: &[i32], live: &T, now: Instant) -> Option<ElementRef> {
        let id = self.id_for(runtime_id)?;
        let entry = self.entries.get_mut(&id)?;
        entry.live = live.clone();
        entry.last_active = now;
        Some(id)
    }

    pub(crate) fn insert_lookup(&mut self, sighting: Sighting<T>, now: Instant) -> ElementRef {
        self.insert(sighting, Origin::Lookup, now)
    }

    pub(crate) fn resolve(&mut self, id: ElementRef, now: Instant) -> Result<Known<T>, Missing> {
        self.check_issued(id)?;
        let entry = self.entries.get_mut(&id).ok_or(Missing::Gone)?;
        entry.last_active = now;
        Ok(Known { live: entry.live.clone(), fingerprint: entry.fingerprint.clone() })
    }

    pub(crate) fn depth(&self, id: ElementRef) -> u16 {
        self.entries.get(&id).map_or(0, |entry| entry.fingerprint.depth())
    }

    pub(crate) fn update_fingerprint(&mut self, id: ElementRef, fingerprint: Fingerprint) {
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.fingerprint = fingerprint;
        }
    }

    pub(crate) fn release_runtime_id(&mut self, id: ElementRef) {
        let Some(entry) = self.entries.get_mut(&id) else { return };
        let runtime_id = mem::take(&mut entry.runtime_id);
        self.forget_runtime_id(id, &runtime_id);
    }

    fn check_issued(&self, id: ElementRef) -> Result<(), Missing> {
        if id.0 == 0 || id.0 >= self.next_ref {
            return Err(Missing::NeverIssued);
        }
        if id.0 < self.first_ref {
            return Err(Missing::FromPreviousWorker);
        }
        Ok(())
    }

    fn forget_runtime_id(&mut self, id: ElementRef, runtime_id: &[i32]) {
        if self.by_runtime.get(runtime_id) == Some(&id) {
            self.by_runtime.remove(runtime_id);
        }
    }

    fn id_for(&self, runtime_id: &[i32]) -> Option<ElementRef> {
        if runtime_id.is_empty() {
            return None;
        }
        self.by_runtime.get(runtime_id).copied().filter(|id| self.entries.contains_key(id))
    }

    fn insert(&mut self, sighting: Sighting<T>, origin: Origin, now: Instant) -> ElementRef {
        let id = ElementRef(self.next_ref);
        self.next_ref = self.next_ref.saturating_add(1);
        if !sighting.runtime_id.is_empty() {
            self.by_runtime.insert(sighting.runtime_id.clone(), id);
        }
        let entry = Entry {
            live: sighting.live,
            runtime_id: sighting.runtime_id,
            fingerprint: sighting.fingerprint,
            origin,
            last_active: now,
        };
        self.entries.insert(id, entry);
        id
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

#[cfg(test)]
mod tests {
    use winpilot_core::Role;

    use super::*;
    use crate::fingerprint::tests::{print, test_window};

    pub(super) const WINDOW: WindowRef = WindowRef(7);

    pub(super) fn sighting_at(live: u32, runtime: i32, name: &str, path: &[u32]) -> Sighting<u32> {
        Sighting { live, runtime_id: vec![42, runtime], fingerprint: print(Role::Button, name, "", "Button", path) }
    }

    pub(super) fn sighting(live: u32, runtime: i32, name: &str) -> Sighting<u32> {
        sighting_at(live, runtime, name, &[0])
    }

    pub(super) fn snapshot(registry: &mut Registry<u32>, runtimes: &[i32], now: Instant) -> Vec<ElementRef> {
        let generation = registry.begin_snapshot(WINDOW);
        let ids = runtimes.iter().map(|r| registry.record(sighting(0, *r, "Save"), generation, now)).collect();
        registry.evict(now, |_| true);
        ids
    }

    #[test]
    fn same_runtime_id_keeps_its_ref() {
        let mut registry = Registry::new(1);
        let now = Instant::now();
        let first = snapshot(&mut registry, &[1, 2, 3], now);
        let second = snapshot(&mut registry, &[3, 2, 1, 4], now);
        assert_eq!(first, [ElementRef(1), ElementRef(2), ElementRef(3)]);
        assert_eq!(second, [ElementRef(3), ElementRef(2), ElementRef(1), ElementRef(4)]);
    }

    #[test]
    fn record_refreshes_the_live_handle() {
        let mut registry = Registry::new(1);
        let now = Instant::now();
        let generation = registry.begin_snapshot(WINDOW);
        let id = registry.record(sighting(10, 1, "Save"), generation, now);
        registry.record(sighting(11, 1, "Saved"), generation, now);
        let known = registry.resolve(id, now).unwrap();
        assert_eq!(known.live, 11);
        assert_eq!(known.fingerprint.name, "Saved");
    }

    #[test]
    fn unknown_and_evicted_refs_differ() {
        let mut registry = Registry::new(1);
        let now = Instant::now();
        snapshot(&mut registry, &[1], now);
        registry.evict(now + eviction::MAX_IDLE, |_| true);
        assert_eq!(registry.resolve(ElementRef(1), now).err(), Some(Missing::Gone));
        assert_eq!(registry.resolve(ElementRef(2), now).err(), Some(Missing::NeverIssued));
        assert_eq!(registry.resolve(ElementRef(0), now).err(), Some(Missing::NeverIssued));
    }

    #[test]
    fn refs_start_at_first_ref() {
        let mut registry = Registry::new(2_000_001);
        let now = Instant::now();
        let ids = snapshot(&mut registry, &[1, 2], now);
        assert_eq!(ids, [ElementRef(2_000_001), ElementRef(2_000_002)]);
        assert_eq!(registry.next_ref(), 2_000_003);
        assert!(registry.resolve(ids[0], now).is_ok());
    }

    #[test]
    fn refs_below_first_ref_came_from_a_previous_worker() {
        let mut registry: Registry<u32> = Registry::new(1_000_001);
        let now = Instant::now();
        assert_eq!(registry.resolve(ElementRef(12), now).err(), Some(Missing::FromPreviousWorker));
        assert_eq!(registry.resolve(ElementRef(1_000_000), now).err(), Some(Missing::FromPreviousWorker));
        assert_eq!(registry.resolve(ElementRef(1_000_001), now).err(), Some(Missing::NeverIssued));
        assert_eq!(registry.resolve(ElementRef(0), now).err(), Some(Missing::NeverIssued));
    }

    #[test]
    fn first_ref_zero_still_starts_at_one() {
        let mut registry = Registry::new(0);
        assert_eq!(snapshot(&mut registry, &[1], Instant::now()), [ElementRef(1)]);
    }

    #[test]
    fn lookups_reuse_snapshot_refs_and_keep_their_fingerprint() {
        let mut registry = Registry::new(1);
        let now = Instant::now();
        let ids = snapshot(&mut registry, &[1, 2], now);
        assert_eq!(registry.revisit(&[42, 2], &5, now), Some(ids[1]));
        let known = registry.resolve(ids[1], now).unwrap();
        assert_eq!(known.live, 5);
        assert_eq!(known.fingerprint.window.hwnd, WINDOW);
        assert_eq!(registry.revisit(&[42, 3], &6, now), None);
        assert_eq!(registry.revisit(&[], &6, now), None);
    }

    #[test]
    fn lookup_entries_keep_their_window() {
        let mut registry = Registry::new(1);
        let now = Instant::now();
        let mut found = sighting(5, 9, "Save");
        found.fingerprint.window = test_window(99);
        let id = registry.insert_lookup(found, now);
        assert_eq!(registry.resolve(id, now).unwrap().fingerprint.window.hwnd, WindowRef(99));
        assert_eq!(registry.revisit(&[42, 9], &6, now), Some(id));
    }

    #[test]
    fn released_runtime_ids_get_a_fresh_ref() {
        let mut registry = Registry::new(1);
        let now = Instant::now();
        let old = snapshot(&mut registry, &[1], now)[0];
        registry.release_runtime_id(old);
        let again = snapshot(&mut registry, &[1], now)[0];
        assert_ne!(again, old);
        assert!(registry.resolve(old, now).is_ok());
        assert_eq!(registry.revisit(&[42, 1], &3, now), Some(again));
    }

    #[test]
    fn elements_without_runtime_ids_get_fresh_refs() {
        let mut registry: Registry<u32> = Registry::new(1);
        let now = Instant::now();
        let generation = registry.begin_snapshot(WINDOW);
        let blank = || Sighting { live: 0, runtime_id: Vec::new(), fingerprint: print(Role::Pane, "", "", "", &[]) };
        let first = registry.record(blank(), generation, now);
        let second = registry.record(blank(), generation, now);
        assert_ne!(first, second);
    }
}
