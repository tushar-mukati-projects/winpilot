use std::{
    collections::HashMap,
    mem,
    time::{Duration, Instant},
};

use deckhand_core::{ElementRef, WindowRef};

use crate::fingerprint::{self, Fingerprint};

const RECENT_SNAPSHOTS: u64 = 3;
const RESOLVE_GRACE: Duration = Duration::from_secs(60);
const MAX_ENTRIES: usize = 50_000;

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
    Gone,
}

struct Entry<T> {
    live: T,
    runtime_id: Vec<i32>,
    fingerprint: Fingerprint,
    generation: Option<u64>,
    last_seen: Instant,
    last_resolved: Option<Instant>,
}

impl<T> Entry<T> {
    fn last_activity(&self) -> Instant {
        self.last_resolved.map_or(self.last_seen, |resolved| resolved.max(self.last_seen))
    }

    fn is_recent(&self, generations: &HashMap<WindowRef, u64>) -> bool {
        let Some(seen) = self.generation else { return false };
        let current = generations.get(&self.fingerprint.window).copied().unwrap_or(seen);
        current.saturating_sub(seen) < RECENT_SNAPSHOTS
    }

    fn resolved_lately(&self, now: Instant) -> bool {
        self.last_resolved.is_some_and(|at| now.saturating_duration_since(at) < RESOLVE_GRACE)
    }
}

pub(crate) struct Registry<T> {
    next_id: u32,
    entries: HashMap<ElementRef, Entry<T>>,
    by_runtime: HashMap<Vec<i32>, ElementRef>,
    generations: HashMap<WindowRef, u64>,
}

impl<T: Clone> Registry<T> {
    pub(crate) fn new() -> Self {
        Self { next_id: 1, entries: HashMap::new(), by_runtime: HashMap::new(), generations: HashMap::new() }
    }

    pub(crate) fn begin_snapshot(&mut self, window: WindowRef) -> u64 {
        let generation = self.generations.entry(window).or_insert(0);
        *generation += 1;
        *generation
    }

    pub(crate) fn generation(&self, window: WindowRef) -> u64 {
        self.generations.get(&window).copied().unwrap_or(0)
    }

    pub(crate) fn watermark(&self) -> u32 {
        self.next_id
    }

    pub(crate) fn record(&mut self, sighting: Sighting<T>, generation: u64, now: Instant) -> ElementRef {
        let Some(id) = self.id_for(&sighting.runtime_id) else {
            return self.insert(sighting, Some(generation), None, now);
        };
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.live = sighting.live;
            entry.fingerprint = sighting.fingerprint;
            entry.generation = Some(generation);
            entry.last_seen = now;
        }
        id
    }

    pub(crate) fn revisit(&mut self, runtime_id: &[i32], live: &T, now: Instant) -> Option<ElementRef> {
        let id = self.id_for(runtime_id)?;
        let entry = self.entries.get_mut(&id)?;
        entry.live = live.clone();
        entry.last_resolved = Some(now);
        Some(id)
    }

    pub(crate) fn insert_lookup(&mut self, sighting: Sighting<T>, now: Instant) -> ElementRef {
        self.insert(sighting, None, Some(now), now)
    }

    pub(crate) fn resolve(&mut self, id: ElementRef, now: Instant) -> Result<Known<T>, Missing> {
        if id.0 == 0 || id.0 >= self.next_id {
            return Err(Missing::NeverIssued);
        }
        let entry = self.entries.get_mut(&id).ok_or(Missing::Gone)?;
        entry.last_resolved = Some(now);
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

    pub(crate) fn seen_in(&self, id: ElementRef, generation: u64) -> bool {
        self.entries.get(&id).is_some_and(|entry| entry.generation == Some(generation))
    }

    pub(crate) fn best_match(&self, target: &Fingerprint, generation: u64, exclude: ElementRef) -> Option<ElementRef> {
        let candidates = self
            .entries
            .iter()
            .filter(|(id, entry)| **id != exclude && entry.generation == Some(generation))
            .map(|(id, entry)| (*id, &entry.fingerprint));
        fingerprint::best_match(target, candidates)
    }

    pub(crate) fn repoint(&mut self, stale: ElementRef, replacement: ElementRef, issued_before: u32) -> bool {
        let Some(source) = self.entries.get(&replacement) else { return false };
        let live = source.live.clone();
        let runtime_id = source.runtime_id.clone();
        let fingerprint = source.fingerprint.clone();
        let generation = source.generation;
        let Some(target) = self.entries.get_mut(&stale) else { return false };
        let old_runtime_id = mem::replace(&mut target.runtime_id, runtime_id.clone());
        target.live = live;
        target.fingerprint = fingerprint;
        target.generation = generation;
        self.forget_runtime_id(stale, &old_runtime_id);
        if !runtime_id.is_empty() {
            self.by_runtime.insert(runtime_id, stale);
        }
        if replacement.0 >= issued_before {
            self.entries.remove(&replacement);
        }
        true
    }

    pub(crate) fn evict(&mut self, now: Instant) {
        self.drop_forgotten(now);
        self.enforce_cap();
    }

    pub(crate) fn enforce_cap(&mut self) {
        let excess = self.entries.len().saturating_sub(MAX_ENTRIES);
        if excess == 0 {
            return;
        }
        let mut by_age: Vec<(Instant, ElementRef)> =
            self.entries.iter().map(|(id, entry)| (entry.last_activity(), *id)).collect();
        by_age.sort_unstable();
        for (_, id) in by_age.into_iter().take(excess) {
            if let Some(entry) = self.entries.remove(&id) {
                self.forget_runtime_id(id, &entry.runtime_id);
            }
        }
    }

    fn drop_forgotten(&mut self, now: Instant) {
        let generations = &self.generations;
        let mut dropped = Vec::new();
        self.entries.retain(|id, entry| {
            let keep = entry.is_recent(generations) || entry.resolved_lately(now);
            if !keep {
                dropped.push((*id, mem::take(&mut entry.runtime_id)));
            }
            keep
        });
        for (id, runtime_id) in dropped {
            self.forget_runtime_id(id, &runtime_id);
        }
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

    fn insert(
        &mut self,
        sighting: Sighting<T>,
        generation: Option<u64>,
        resolved: Option<Instant>,
        now: Instant,
    ) -> ElementRef {
        let id = ElementRef(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        if !sighting.runtime_id.is_empty() {
            self.by_runtime.insert(sighting.runtime_id.clone(), id);
        }
        let entry = Entry {
            live: sighting.live,
            runtime_id: sighting.runtime_id,
            fingerprint: sighting.fingerprint,
            generation,
            last_seen: now,
            last_resolved: resolved,
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
    use deckhand_core::Role;

    use super::*;
    use crate::fingerprint::tests::print;

    fn sighting(live: u32, runtime: i32, name: &str) -> Sighting<u32> {
        Sighting { live, runtime_id: vec![42, runtime], fingerprint: print(Role::Button, name, "", "Button", &[0]) }
    }

    fn snapshot(registry: &mut Registry<u32>, runtimes: &[i32], now: Instant) -> Vec<ElementRef> {
        let generation = registry.begin_snapshot(WindowRef(7));
        let ids = runtimes.iter().map(|r| registry.record(sighting(0, *r, "Save"), generation, now)).collect();
        registry.evict(now);
        ids
    }

    #[test]
    fn same_runtime_id_keeps_its_ref() {
        let mut registry = Registry::new();
        let now = Instant::now();
        let first = snapshot(&mut registry, &[1, 2, 3], now);
        let second = snapshot(&mut registry, &[3, 2, 1, 4], now);
        assert_eq!(first, [ElementRef(1), ElementRef(2), ElementRef(3)]);
        assert_eq!(second, [ElementRef(3), ElementRef(2), ElementRef(1), ElementRef(4)]);
    }

    #[test]
    fn record_refreshes_the_live_handle() {
        let mut registry = Registry::new();
        let now = Instant::now();
        let generation = registry.begin_snapshot(WindowRef(7));
        let id = registry.record(sighting(10, 1, "Save"), generation, now);
        registry.record(sighting(11, 1, "Saved"), generation, now);
        let known = registry.resolve(id, now).unwrap();
        assert_eq!(known.live, 11);
        assert_eq!(known.fingerprint.name, "Saved");
    }

    #[test]
    fn unknown_and_evicted_refs_differ() {
        let mut registry = Registry::new();
        let now = Instant::now();
        snapshot(&mut registry, &[1], now);
        for _ in 0..3 {
            snapshot(&mut registry, &[], now);
        }
        assert_eq!(registry.resolve(ElementRef(1), now).err(), Some(Missing::Gone));
        assert_eq!(registry.resolve(ElementRef(2), now).err(), Some(Missing::NeverIssued));
        assert_eq!(registry.resolve(ElementRef(0), now).err(), Some(Missing::NeverIssued));
    }

    #[test]
    fn entries_survive_three_snapshots() {
        let mut registry = Registry::new();
        let now = Instant::now();
        snapshot(&mut registry, &[1], now);
        snapshot(&mut registry, &[], now);
        snapshot(&mut registry, &[], now);
        assert_eq!(registry.len(), 1);
        snapshot(&mut registry, &[], now);
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn resolved_entries_survive_for_a_minute() {
        let mut registry = Registry::new();
        let now = Instant::now();
        let ids = snapshot(&mut registry, &[1], now);
        registry.resolve(ids[0], now).unwrap();
        for _ in 0..5 {
            snapshot(&mut registry, &[], now + Duration::from_secs(30));
        }
        assert_eq!(registry.len(), 1);
        snapshot(&mut registry, &[], now + Duration::from_secs(61));
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn lookups_live_only_by_resolution() {
        let mut registry = Registry::new();
        let now = Instant::now();
        let id = registry.insert_lookup(sighting(5, 9, "Save"), now);
        registry.evict(now + Duration::from_secs(59));
        assert!(registry.resolve(id, now + Duration::from_secs(59)).is_ok());
        registry.evict(now + Duration::from_secs(120));
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn lookups_reuse_snapshot_refs_and_keep_their_fingerprint() {
        let mut registry = Registry::new();
        let now = Instant::now();
        let ids = snapshot(&mut registry, &[1, 2], now);
        assert_eq!(registry.revisit(&[42, 2], &5, now), Some(ids[1]));
        let known = registry.resolve(ids[1], now).unwrap();
        assert_eq!(known.live, 5);
        assert_eq!(known.fingerprint.window, WindowRef(7));
        assert_eq!(registry.revisit(&[42, 3], &6, now), None);
        assert_eq!(registry.revisit(&[], &6, now), None);
    }

    #[test]
    fn lookup_entries_keep_their_window() {
        let mut registry = Registry::new();
        let now = Instant::now();
        let mut found = sighting(5, 9, "Save");
        found.fingerprint.window = WindowRef(99);
        let id = registry.insert_lookup(found, now);
        assert_eq!(registry.resolve(id, now).unwrap().fingerprint.window, WindowRef(99));
        assert_eq!(registry.revisit(&[42, 9], &6, now), Some(id));
    }

    #[test]
    fn cap_evicts_the_oldest() {
        let mut registry = Registry::new();
        let start = Instant::now();
        let generation = registry.begin_snapshot(WindowRef(7));
        let total = MAX_ENTRIES + 5;
        for index in 0..total {
            let at = start + Duration::from_millis(index as u64);
            registry.record(sighting(0, index as i32, "Save"), generation, at);
        }
        registry.enforce_cap();
        assert_eq!(registry.len(), MAX_ENTRIES);
        assert_eq!(registry.resolve(ElementRef(5), start).err(), Some(Missing::Gone));
        assert!(registry.resolve(ElementRef(6), start).is_ok());
    }

    #[test]
    fn repoint_moves_the_runtime_id_and_drops_the_fresh_duplicate() {
        let mut registry = Registry::new();
        let now = Instant::now();
        let old = snapshot(&mut registry, &[1], now)[0];
        let issued_before = registry.watermark();
        let generation = registry.begin_snapshot(WindowRef(7));
        let fresh = registry.record(sighting(77, 2, "Save"), generation, now);
        assert!(registry.repoint(old, fresh, issued_before));
        assert_eq!(registry.resolve(old, now).unwrap().live, 77);
        assert_eq!(registry.resolve(fresh, now).err(), Some(Missing::Gone));
        let again = registry.begin_snapshot(WindowRef(7));
        assert_eq!(registry.record(sighting(78, 2, "Save"), again, now), old);
    }

    #[test]
    fn best_match_only_considers_the_latest_snapshot() {
        let mut registry = Registry::new();
        let now = Instant::now();
        let stale = snapshot(&mut registry, &[1], now)[0];
        let fingerprint = registry.resolve(stale, now).unwrap().fingerprint;
        let generation = registry.begin_snapshot(WindowRef(7));
        let fresh = registry.record(sighting(3, 2, "Save"), generation, now);
        assert_eq!(registry.best_match(&fingerprint, generation, stale), Some(fresh));
        assert_eq!(registry.best_match(&fingerprint, generation + 1, stale), None);
        assert!(registry.seen_in(fresh, generation));
        assert!(!registry.seen_in(stale, generation));
    }

    #[test]
    fn elements_without_runtime_ids_get_fresh_refs() {
        let mut registry: Registry<u32> = Registry::new();
        let now = Instant::now();
        let generation = registry.begin_snapshot(WindowRef(7));
        let blank = || Sighting { live: 0, runtime_id: Vec::new(), fingerprint: print(Role::Pane, "", "", "", &[]) };
        let first = registry.record(blank(), generation, now);
        let second = registry.record(blank(), generation, now);
        assert_ne!(first, second);
    }
}
