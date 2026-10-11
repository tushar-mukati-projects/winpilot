use std::{
    collections::HashSet,
    mem,
    time::{Duration, Instant},
};

use deckhand_core::{ElementRef, WindowRef};

use super::{Entry, Origin, Registry};

const SNAPSHOT_CALLS_KEPT: u64 = 3;
const ACTIVITY_GRACE: Duration = Duration::from_secs(60);
pub(super) const MAX_IDLE: Duration = Duration::from_secs(10 * 60);
const MAX_ENTRIES: usize = 50_000;

impl<T> Entry<T> {
    fn worth_keeping(&self, current_generation: Option<u64>, now: Instant) -> bool {
        let idle = now.saturating_duration_since(self.last_active);
        idle < MAX_IDLE && (idle < ACTIVITY_GRACE || self.in_recent_snapshot(current_generation))
    }

    fn in_recent_snapshot(&self, current_generation: Option<u64>) -> bool {
        let Origin::Snapshot(seen) = self.origin else { return false };
        current_generation.unwrap_or(seen).saturating_sub(seen) < SNAPSHOT_CALLS_KEPT
    }
}

impl<T: Clone> Registry<T> {
    pub(crate) fn evict(&mut self, now: Instant, window_alive: impl FnMut(WindowRef) -> bool) {
        let closed = self.closed_windows(window_alive);
        self.generations.retain(|window, _| !closed.contains(window));
        self.drop_unwanted(&closed, now);
        self.enforce_cap();
    }

    pub(crate) fn enforce_cap(&mut self) {
        let excess = self.entries.len().saturating_sub(MAX_ENTRIES);
        if excess == 0 {
            return;
        }
        let mut by_age: Vec<(Instant, ElementRef)> =
            self.entries.iter().map(|(id, entry)| (entry.last_active, *id)).collect();
        by_age.sort_unstable();
        for (_, id) in by_age.into_iter().take(excess) {
            if let Some(entry) = self.entries.remove(&id) {
                self.forget_runtime_id(id, &entry.runtime_id);
            }
        }
    }

    fn closed_windows(&self, mut window_alive: impl FnMut(WindowRef) -> bool) -> HashSet<WindowRef> {
        let windows: HashSet<WindowRef> = self
            .entries
            .values()
            .map(|entry| entry.fingerprint.window.hwnd)
            .chain(self.generations.keys().copied())
            .filter(|window| window.0 != 0)
            .collect();
        windows.into_iter().filter(|window| !window_alive(*window)).collect()
    }

    fn drop_unwanted(&mut self, closed: &HashSet<WindowRef>, now: Instant) {
        let generations = &self.generations;
        let mut dropped = Vec::new();
        self.entries.retain(|id, entry| {
            let window = entry.fingerprint.window.hwnd;
            let keep = !closed.contains(&window) && entry.worth_keeping(generations.get(&window).copied(), now);
            if !keep {
                dropped.push((*id, mem::take(&mut entry.runtime_id)));
            }
            keep
        });
        for (id, runtime_id) in dropped {
            self.forget_runtime_id(id, &runtime_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        registry::{
            Missing,
            tests::{WINDOW, sighting, snapshot},
        },
        window_identity::WindowIdentity,
    };

    fn seconds(count: u64) -> Duration {
        Duration::from_secs(count)
    }

    #[test]
    fn entries_survive_three_snapshot_calls_once_the_grace_has_passed() {
        let mut registry = Registry::new(1);
        let start = Instant::now();
        snapshot(&mut registry, &[1], start);
        snapshot(&mut registry, &[], start + seconds(61));
        snapshot(&mut registry, &[], start + seconds(61));
        assert_eq!(registry.len(), 1);
        snapshot(&mut registry, &[], start + seconds(61));
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn recently_seen_entries_survive_any_number_of_snapshot_calls() {
        let mut registry = Registry::new(1);
        let start = Instant::now();
        snapshot(&mut registry, &[1], start);
        for _ in 0..5 {
            snapshot(&mut registry, &[], start + seconds(30));
        }
        assert_eq!(registry.len(), 1);
        snapshot(&mut registry, &[], start + seconds(61));
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn resolving_restarts_the_grace() {
        let mut registry = Registry::new(1);
        let start = Instant::now();
        let ids = snapshot(&mut registry, &[1], start);
        registry.resolve(ids[0], start + seconds(20)).unwrap();
        for _ in 0..5 {
            snapshot(&mut registry, &[], start + seconds(50));
        }
        assert_eq!(registry.len(), 1);
        snapshot(&mut registry, &[], start + seconds(81));
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn idle_entries_expire_after_ten_minutes_even_without_new_snapshots() {
        let mut registry = Registry::new(1);
        let start = Instant::now();
        let id = snapshot(&mut registry, &[1], start)[0];
        registry.evict(start + MAX_IDLE.saturating_sub(seconds(1)), |_| true);
        assert_eq!(registry.len(), 1);
        registry.evict(start + MAX_IDLE, |_| true);
        assert_eq!(registry.len(), 0);
        assert_eq!(registry.resolve(id, start).err(), Some(Missing::Gone));
    }

    #[test]
    fn lookups_live_only_by_activity() {
        let mut registry = Registry::new(1);
        let start = Instant::now();
        let id = registry.insert_lookup(sighting(5, 9, "Save"), start);
        registry.evict(start + seconds(59), |_| true);
        assert!(registry.resolve(id, start + seconds(59)).is_ok());
        registry.evict(start + seconds(118), |_| true);
        assert_eq!(registry.len(), 1);
        registry.evict(start + seconds(120), |_| true);
        assert_eq!(registry.len(), 0);
    }

    #[test]
    fn entries_of_closed_windows_are_dropped_at_once() {
        let mut registry = Registry::new(1);
        let now = Instant::now();
        let id = snapshot(&mut registry, &[1, 2], now)[0];
        registry.begin_snapshot(WINDOW);
        let mut checked = Vec::new();
        registry.evict(now, |window| {
            checked.push(window);
            window != WINDOW
        });
        assert_eq!(checked, [WINDOW]);
        assert_eq!(registry.len(), 0);
        assert_eq!(registry.resolve(id, now).err(), Some(Missing::Gone));
        assert_eq!(registry.begin_snapshot(WINDOW), 1);
        assert_eq!(registry.revisit(&[42, 1], &0, now), None);
    }

    #[test]
    fn entries_without_a_window_are_not_checked() {
        let mut registry = Registry::new(1);
        let now = Instant::now();
        let mut found = sighting(5, 9, "Save");
        found.fingerprint.window = WindowIdentity::unknown();
        registry.insert_lookup(found, now);
        registry.evict(now, |_| false);
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn cap_evicts_the_oldest() {
        let mut registry = Registry::new(1);
        let start = Instant::now();
        let generation = registry.begin_snapshot(WINDOW);
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
}
