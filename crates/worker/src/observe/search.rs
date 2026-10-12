use std::time::{Duration, Instant};

use deckhand_core::{
    Result, Snapshot, WindowInfo,
    protocol::{Found, SearchReport, SearchRequest},
    selector::Selector,
};

use crate::{app_windows::app_windows_matching, call_scope::CallScope, engine::Engine};

const PATH_DEPTH: usize = 4;
const SEARCH_BUDGET: Duration = Duration::from_secs(5);

pub(crate) fn run(engine: &Engine, request: &SearchRequest, call: &CallScope) -> Result<SearchReport> {
    let selector: Selector = request.query.parse()?;
    let mut matches = Vec::new();
    let complete = search_every_window(engine, &selector, call, request.max, &mut matches)?;
    let truncated = matches.len() > request.max || !complete;
    matches.truncate(request.max);
    Ok(SearchReport { matches, truncated })
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

fn time_for_another(spent: Duration, call_remaining: Duration, snapshot_budget: Duration) -> bool {
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
        .map(|ancestor| ancestor.name.trim())
        .filter(|name| !name.is_empty() && *name != snapshot.window.title.trim())
        .map(str::to_string)
        .collect();
    let skip = names.len().saturating_sub(PATH_DEPTH);
    names.into_iter().skip(skip).collect()
}

#[cfg(test)]
mod tests {
    use deckhand_core::Role;

    use super::*;
    use crate::test_fixture::{element, snapshot};

    #[test]
    fn paths_skip_unnamed_ancestors_and_the_window_title() {
        let group = |id: u32, depth: u16, name: &str| element(id, depth, Role::Group, name);
        let snapshot =
            snapshot("App", vec![group(1, 0, "App"), group(2, 1, ""), group(3, 2, "Toolbar"), group(4, 3, "Save")]);
        assert_eq!(ancestor_names(&snapshot, 3), ["Toolbar"]);
    }

    #[test]
    fn the_search_stops_when_time_runs_short() {
        let budget = Duration::from_millis(2500);
        assert!(time_for_another(Duration::ZERO, Duration::from_secs(9), budget));
        assert!(!time_for_another(SEARCH_BUDGET, Duration::from_secs(9), budget));
        assert!(!time_for_another(Duration::ZERO, Duration::from_secs(2), budget));
    }
}
