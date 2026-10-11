use std::time::{Duration, Instant};

use deckhand_core::{
    Result, Snapshot, WindowInfo,
    protocol::{FindArgs, FindReport, Found},
    selector::Selector,
};

use crate::{app_windows::app_windows_matching, call_scope::CallScope, engine::Engine, resolve};

const DEFAULT_MAX: usize = 20;
const PATH_DEPTH: usize = 4;
const SEARCH_BUDGET: Duration = Duration::from_secs(5);

pub fn run(engine: &Engine, args: &FindArgs, call: &CallScope) -> Result<FindReport> {
    let selector: Selector = args.query.parse()?;
    let max = args.max.map_or(DEFAULT_MAX, |max| max.max(1) as usize);
    let mut matches = Vec::new();
    let mut complete = true;
    if args.all_windows {
        complete = search_every_window(engine, &selector, call, max, &mut matches)?;
    } else {
        let window = resolve::window(engine, args.window.as_deref())?;
        collect(&engine.snapshot(&window)?, &selector, &window, &mut matches);
    }
    let truncated = matches.len() > max || !complete;
    matches.truncate(max);
    Ok(FindReport { matches, truncated })
}

fn search_every_window(
    engine: &Engine,
    selector: &Selector,
    call: &CallScope,
    max: usize,
    matches: &mut Vec<Found>,
) -> Result<bool> {
    let started = Instant::now();
    for window in app_windows_matching(engine, selector)? {
        if matches.len() > max {
            return Ok(true);
        }
        if !time_for_another(started.elapsed(), call.remaining(), engine.snapshot_budget()) {
            return Ok(false);
        }
        let Ok(snapshot) = engine.snapshot(&window) else { continue };
        collect(&snapshot, selector, &window, matches);
    }
    Ok(true)
}

pub fn time_for_another(spent: Duration, call_remaining: Duration, snapshot_budget: Duration) -> bool {
    spent < SEARCH_BUDGET && call_remaining > snapshot_budget
}

fn collect(snapshot: &Snapshot, selector: &Selector, window: &WindowInfo, matches: &mut Vec<Found>) {
    for element in selector.find(snapshot) {
        let path = snapshot.index_of(element.id).map(|index| ancestor_names(snapshot, index)).unwrap_or_default();
        matches.push(Found { element: element.clone(), window: window.clone(), path });
    }
}

fn ancestor_names(snapshot: &Snapshot, index: usize) -> Vec<String> {
    let names: Vec<String> = snapshot
        .ancestors(index)
        .into_iter()
        .filter(|ancestor| !ancestor.name.trim().is_empty())
        .map(|ancestor| ancestor.name.clone())
        .collect();
    let skip = names.len().saturating_sub(PATH_DEPTH);
    names.into_iter().skip(skip).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_search_stops_when_time_runs_short() {
        let budget = Duration::from_millis(2500);
        assert!(time_for_another(Duration::ZERO, Duration::from_secs(9), budget));
        assert!(!time_for_another(SEARCH_BUDGET, Duration::from_secs(9), budget));
        assert!(!time_for_another(Duration::ZERO, Duration::from_secs(2), budget));
    }
}
