use std::time::{Duration, Instant};

use deckhand_core::{
    Result, Snapshot, WindowInfo,
    protocol::{FindArgs, FindReport, Found},
    selector::Selector,
};

use crate::{app_windows::app_windows_matching, call_scope::CallScope, engine::Engine, overlay, resolve};

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
        overlay::scan(engine, &window);
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
        .map(|ancestor| ancestor.name.trim())
        .filter(|name| !name.is_empty() && *name != snapshot.window.title.trim())
        .map(str::to_string)
        .collect();
    let skip = names.len().saturating_sub(PATH_DEPTH);
    names.into_iter().skip(skip).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_skip_unnamed_ancestors_and_the_window_title() {
        use deckhand_core::{ElementRef, Rect, Role, WindowRef, WindowState};
        let window = WindowInfo {
            hwnd: WindowRef(1),
            title: "App".to_string(),
            process: "app.exe".to_string(),
            pid: 1,
            class_name: String::new(),
            rect: Rect::new(0, 0, 100, 100),
            state: WindowState::Normal,
            foreground: true,
            elevated: false,
        };
        let element = |id: u32, depth: u16, name: &str| deckhand_core::Element {
            id: ElementRef(id),
            depth,
            role: Role::Group,
            name: name.to_string(),
            value: None,
            automation_id: String::new(),
            class_name: String::new(),
            rect: Rect::new(0, 0, 10, 10),
            state: deckhand_core::ElementState::default(),
            patterns: deckhand_core::Patterns::default(),
        };
        let snapshot = Snapshot {
            window,
            elements: vec![element(1, 0, "App"), element(2, 1, ""), element(3, 2, "Toolbar"), element(4, 3, "Save")],
            focused: None,
            truncated: false,
        };
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
