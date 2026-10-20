use std::time::Instant;

use winpilot_core::{
    Element, ElementRef, Error, Result, Target, WindowInfo, WindowRef, WindowState, ocr::exposes_no_uia,
    selector::Selector,
};

use super::{
    check::{Presence, Shown, unless_gone},
    ocr_look::OcrLook,
};
use crate::{app_windows::app_windows_matching, engine::Engine, resolve, watch::Watch};

pub(crate) fn locate(
    engine: &Engine,
    target: &Target,
    scope: Option<&str>,
    ocr: &mut OcrLook<'_>,
    ends: Instant,
) -> Result<Presence> {
    match target {
        Target::Element(id) => element(engine, *id),
        Target::Window(hwnd) => window(*hwnd),
        Target::Selector(selector) => selector_match(engine, selector, scope, ocr, ends),
        Target::Point { .. } => Err(Error::invalid("wait needs an element, a window or a selector, not a point")),
    }
}

pub(crate) fn watch(engine: &Engine, target: &Target, scope: Option<&str>) -> Watch {
    let windows = match target {
        Target::Element(id) => resolve::element_window(engine, *id).map(|window| vec![window]),
        Target::Window(hwnd) => winpilot_desktop::window(*hwnd).map(|window| vec![window]),
        Target::Selector(selector) => scope_windows(engine, selector, scope),
        Target::Point { .. } => Ok(Vec::new()),
    };
    windows.map(|windows| Watch::windows(&windows)).unwrap_or_default()
}

fn any_visible(found: &[&Element]) -> bool {
    found.iter().any(|element| element.is_visible())
}

fn is_open(window: &WindowInfo) -> bool {
    window.state != WindowState::Minimized
}

fn element(engine: &Engine, id: ElementRef) -> Result<Presence> {
    let found = unless_gone(engine.uia.run(move |uia| uia.element(id)))?;
    Ok(Presence::certain(found.filter(Element::is_visible).map(|_| Shown::Element)))
}

fn window(hwnd: WindowRef) -> Result<Presence> {
    let found = unless_gone(winpilot_desktop::window(hwnd))?;
    Ok(Presence::certain(found.filter(is_open).map(Shown::Window)))
}

fn selector_match(
    engine: &Engine,
    selector: &Selector,
    scope: Option<&str>,
    ocr: &mut OcrLook<'_>,
    ends: Instant,
) -> Result<Presence> {
    let windows = scope_windows(engine, selector, scope)?;
    if let Some(text) = selector.ocr_text() {
        return ocr.presence(engine, &windows, text, selector.nth());
    }
    if !selector.has_element_terms() {
        return Ok(Presence::certain(windows.into_iter().find(is_open).map(Shown::Window)));
    }
    let here = windows.iter().filter_map(|window| unless_gone(engine.snapshot(window)).transpose());
    let snapshots = resolve::selector_snapshots(engine, selector, here.collect::<Result<_>>()?, ends)?;
    if snapshots.iter().any(|snapshot| any_visible(&selector.find(snapshot))) {
        return Ok(Presence::certain(Some(Shown::Element)));
    }
    let complete = snapshots.iter().all(|snapshot| !snapshot.truncated);
    let blind: Vec<WindowInfo> = snapshots.into_iter().filter(exposes_no_uia).map(|snapshot| snapshot.window).collect();
    match selector.plain_text().filter(|_| !blind.is_empty()) {
        Some(text) => ocr.presence(engine, &blind, text, selector.nth()),
        None => Ok(Presence { shown: None, complete }),
    }
}

fn scope_windows(engine: &Engine, selector: &Selector, scope: Option<&str>) -> Result<Vec<WindowInfo>> {
    if scope.is_none() && selector.has_window_terms() {
        return app_windows_matching(engine, selector);
    }
    Ok(unless_gone(resolve::window(engine, scope))?.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use winpilot_core::{Rect, Role};

    use super::*;

    fn element(id: u32, offscreen: bool) -> Element {
        let mut element = crate::test_fixture::element(id, 1, Role::Button, "OK");
        element.state.offscreen = offscreen;
        element
    }

    #[test]
    fn only_a_visible_match_counts() {
        let hidden = element(1, true);
        let shown = element(2, false);
        assert!(any_visible(&[&hidden, &shown]));
        assert!(!any_visible(&[&hidden]));
        assert!(!any_visible(&[]));
    }

    #[test]
    fn minimized_windows_are_not_open() {
        let mut window = crate::test_fixture::window(1, "a.exe", "A", Rect::new(0, 0, 10, 10));
        assert!(is_open(&window));
        window.state = WindowState::Minimized;
        assert!(!is_open(&window));
    }
}
