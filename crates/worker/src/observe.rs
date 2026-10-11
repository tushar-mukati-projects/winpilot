use std::collections::BTreeSet;

use deckhand_core::{
    Result, Snapshot, WindowInfo,
    protocol::{Observation, ObserveArgs},
    selector::Selector,
};

use crate::{app_windows::app_windows, engine::Engine, overlay, resolve};

pub fn run(engine: &Engine, args: &ObserveArgs) -> Result<Observation> {
    let window = resolve::window(engine, args.window.as_deref())?;
    let query = args.query.as_deref().map(str::parse::<Selector>).transpose()?;
    let mut observation = observe_window(engine, &window)?;
    if let Some(query) = query {
        observation.snapshot = keep_matches_and_ancestors(&observation.snapshot, &query);
    }
    overlay::scan(engine, &window);
    Ok(observation)
}

pub fn observe_window(engine: &Engine, window: &WindowInfo) -> Result<Observation> {
    let snapshot = engine.snapshot(window)?;
    let others = app_windows(engine)?.into_iter().filter(|other| other.hwnd != window.hwnd).collect();
    Ok(Observation { snapshot, others })
}

fn keep_matches_and_ancestors(snapshot: &Snapshot, query: &Selector) -> Snapshot {
    let mut keep = BTreeSet::new();
    for found in query.find(snapshot) {
        let Some(index) = snapshot.index_of(found.id) else { continue };
        keep.insert(index);
        keep.extend(snapshot.ancestors(index).iter().filter_map(|ancestor| snapshot.index_of(ancestor.id)));
    }
    Snapshot {
        window: snapshot.window.clone(),
        elements: keep.into_iter().map(|index| snapshot.elements[index].clone()).collect(),
        focused: snapshot.focused,
        truncated: snapshot.truncated,
    }
}
