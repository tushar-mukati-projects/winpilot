use std::collections::BTreeSet;

use winpilot_core::{
    Result, Snapshot, WindowInfo,
    ocr::exposes_no_uia,
    protocol::{ElementsRequest, Observation},
    selector::Selector,
};

use crate::{
    app_windows::app_windows,
    call_scope::CallScope,
    engine::Engine,
    ocr_read::{Freshness, window_lines},
    overlay, resolve,
};

pub(crate) fn run(engine: &Engine, request: &ElementsRequest, call: &CallScope) -> Result<Observation> {
    let window = resolve::window(engine, request.window.as_deref())?;
    let query = request.query.as_deref().map(str::parse::<Selector>).transpose()?;
    let mut observation = observe_window(engine, &window)?;
    if let Some(query) = query {
        let snapshot = resolve::searched(engine, observation.snapshot, &query, call.deadline())?;
        observation.snapshot = keep_matches_and_ancestors(&snapshot, &query);
    }
    overlay::scan(engine, &window);
    Ok(observation)
}

pub(crate) fn observe_window(engine: &Engine, window: &WindowInfo) -> Result<Observation> {
    let snapshot = engine.snapshot(window)?;
    let ocr =
        exposes_no_uia(&snapshot).then(|| window_lines(engine, window, None, Freshness::CachedIsFine).ok()).flatten();
    let others = app_windows(engine)?.into_iter().filter(|other| other.hwnd != window.hwnd).collect();
    Ok(Observation { snapshot, others, ocr })
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

#[cfg(test)]
mod tests {
    use winpilot_core::Role;

    use super::*;
    use crate::test_fixture::{element, snapshot};

    #[test]
    fn a_query_keeps_matches_with_their_ancestors() {
        let elements = vec![
            element(1, 0, Role::Pane, "Root"),
            element(2, 1, Role::Group, "Toolbar"),
            element(3, 2, Role::Button, "Save"),
            element(4, 1, Role::Button, "Cancel"),
        ];
        let query: Selector = "name:Save".parse().unwrap();
        let kept = keep_matches_and_ancestors(&snapshot("App", elements), &query);
        let names: Vec<&str> = kept.elements.iter().map(|element| element.name.as_str()).collect();
        assert_eq!(names, ["Root", "Toolbar", "Save"]);
    }
}
