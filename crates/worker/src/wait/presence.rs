use deckhand_core::{
    Element, ElementRef, Error, Result, Target, WindowInfo, WindowRef, WindowState, selector::Selector,
};

use super::check::{Presence, Shown, unless_gone};
use crate::{app_windows::app_windows_matching, engine::Engine, resolve, watch::Watch};

pub fn locate(engine: &Engine, target: &Target, scope: Option<&str>) -> Result<Presence> {
    match target {
        Target::Element(id) => element(engine, *id),
        Target::Window(hwnd) => window(*hwnd),
        Target::Selector(selector) => selector_match(engine, selector, scope),
        Target::Point { .. } => Err(Error::invalid("wait_for needs an element, a window or a selector, not a point")),
    }
}

pub fn watch(engine: &Engine, target: &Target, scope: Option<&str>) -> Watch {
    let windows = match target {
        Target::Element(id) => resolve::element_window(engine, *id).map(|window| vec![window]),
        Target::Window(hwnd) => deckhand_desktop::window(*hwnd).map(|window| vec![window]),
        Target::Selector(selector) => scope_windows(engine, selector, scope),
        Target::Point { .. } => Ok(Vec::new()),
    };
    windows.map(|windows| Watch::windows(&windows)).unwrap_or_default()
}

pub fn first_visible<'a>(found: &[&'a Element]) -> Option<&'a Element> {
    found.iter().copied().find(|element| element.is_visible())
}

pub fn is_open(window: &WindowInfo) -> bool {
    window.state != WindowState::Minimized
}

fn element(engine: &Engine, id: ElementRef) -> Result<Presence> {
    let found = unless_gone(engine.uia.run(move |uia| uia.element(id)))?;
    Ok(Presence::certain(found.filter(Element::is_visible).map(Shown::Element)))
}

fn window(hwnd: WindowRef) -> Result<Presence> {
    let found = unless_gone(deckhand_desktop::window(hwnd))?;
    Ok(Presence::certain(found.filter(is_open).map(Shown::Window)))
}

fn selector_match(engine: &Engine, selector: &Selector, scope: Option<&str>) -> Result<Presence> {
    let windows = scope_windows(engine, selector, scope)?;
    if !selector.has_element_terms() {
        return Ok(Presence::certain(windows.into_iter().find(is_open).map(Shown::Window)));
    }
    let mut complete = true;
    for window in windows {
        let Some(snapshot) = unless_gone(engine.snapshot(&window))? else { continue };
        if let Some(element) = first_visible(&selector.find(&snapshot)) {
            return Ok(Presence::certain(Some(Shown::Element(element.clone()))));
        }
        complete &= !snapshot.truncated;
    }
    Ok(Presence { shown: None, complete })
}

fn scope_windows(engine: &Engine, selector: &Selector, scope: Option<&str>) -> Result<Vec<WindowInfo>> {
    if scope.is_none() && selector.has_window_terms() {
        return app_windows_matching(engine, selector);
    }
    Ok(unless_gone(resolve::window(engine, scope))?.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use deckhand_core::{ElementState, Patterns, Rect, Role};

    use super::*;

    fn element(id: u32, offscreen: bool) -> Element {
        Element {
            id: ElementRef(id),
            depth: 1,
            role: Role::Button,
            name: "OK".to_string(),
            value: None,
            automation_id: String::new(),
            class_name: String::new(),
            rect: Rect::new(0, 0, 10, 10),
            state: ElementState { enabled: true, offscreen, ..ElementState::default() },
            patterns: Patterns::default(),
        }
    }

    #[test]
    fn the_first_visible_match_wins() {
        let hidden = element(1, true);
        let shown = element(2, false);
        let later = element(3, false);
        assert_eq!(first_visible(&[&hidden, &shown, &later]).map(|found| found.id), Some(ElementRef(2)));
        assert_eq!(first_visible(&[&hidden]), None);
        assert_eq!(first_visible(&[]), None);
    }

    #[test]
    fn minimized_windows_are_not_open() {
        let mut window = crate::test_fixture::window(1, "a.exe", "A", Rect::new(0, 0, 10, 10));
        assert!(is_open(&window));
        window.state = WindowState::Minimized;
        assert!(!is_open(&window));
    }
}
