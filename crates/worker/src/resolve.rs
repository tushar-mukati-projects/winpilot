use std::time::Duration;

use deckhand_core::{
    Element, ElementRef, Error, ErrorCode, ImageRef, Point, Result, Target, WindowInfo, selector::Selector,
};

use crate::{
    app_windows::{app_windows, app_windows_matching},
    call_scope::CallScope,
    default_window::default_window,
    engine::Engine,
    until::until,
    watch::Watch,
    window_spec::{self, WindowSpec},
};

#[derive(Debug, Clone)]
pub enum Reach {
    Element(Element),
    Point { point: Point, under: Element },
    Window,
}

#[derive(Debug, Clone)]
pub struct Resolved {
    pub window: WindowInfo,
    pub reach: Reach,
}

impl Resolved {
    pub fn element(&self) -> Option<&Element> {
        match &self.reach {
            Reach::Element(element) | Reach::Point { under: element, .. } => Some(element),
            Reach::Window => None,
        }
    }

    pub fn tracked_element(&self) -> Option<&Element> {
        match &self.reach {
            Reach::Element(element) => Some(element),
            Reach::Point { .. } | Reach::Window => None,
        }
    }

    pub fn require_element(&self, step: &str) -> Result<&Element> {
        self.element()
            .ok_or_else(|| Error::invalid(format!("{step} needs an element, not the window {}", self.window.hwnd)))
    }
}

pub fn window(engine: &Engine, spec: Option<&str>) -> Result<WindowInfo> {
    let Some(spec) = spec.map(str::trim).filter(|spec| !spec.is_empty()) else { return default_window(engine) };
    let window = match window_spec::parse(spec)? {
        WindowSpec::Handle(hwnd) => deckhand_desktop::window(hwnd)?,
        WindowSpec::Element(id) => element_window(engine, id)?,
        spec => first_matching(engine, &spec)?,
    };
    engine.guard.check_window(&window)?;
    Ok(window)
}

pub fn windows_matching(engine: &Engine, spec: &str) -> Result<Vec<WindowInfo>> {
    match window_spec::parse(spec)? {
        WindowSpec::Handle(hwnd) => Ok(vec![deckhand_desktop::window(hwnd)?]),
        WindowSpec::Element(id) => Ok(vec![element_window(engine, id)?]),
        spec => Ok(app_windows(engine)?.into_iter().filter(|window| window_spec::matches(&spec, window)).collect()),
    }
}

pub fn element_window(engine: &Engine, id: ElementRef) -> Result<WindowInfo> {
    let hwnd = engine.uia.run(move |uia| uia.window_of(id))?;
    deckhand_desktop::window(hwnd)
}

fn first_matching(engine: &Engine, spec: &WindowSpec) -> Result<WindowInfo> {
    app_windows(engine)?
        .into_iter()
        .find(|window| window_spec::matches(spec, window))
        .ok_or_else(|| Error::not_found(format!("no open window matches {}", window_spec::describe(spec))))
}

pub fn target(engine: &Engine, target: &Target, call: &CallScope) -> Result<Resolved> {
    let Target::Selector(selector) = target else { return resolve_once(engine, target) };
    let patience = Duration::from_millis(engine.config.act.auto_wait_ms);
    let mut missing = None;
    let found = until(engine, call, patience, &selector_watch(engine, selector), || {
        match resolve_selector(engine, selector) {
            Ok(resolved) => Ok(Some(resolved)),
            Err(error) if error.code == ErrorCode::NotFound => {
                missing = Some(error);
                Ok(None)
            }
            Err(error) => Err(error),
        }
    })?;
    found.ok_or_else(|| missing.unwrap_or_else(|| no_window(selector)))
}

fn resolve_once(engine: &Engine, target: &Target) -> Result<Resolved> {
    match target {
        Target::Element(id) => resolve_element(engine, *id),
        Target::Window(hwnd) => resolve_window(engine, deckhand_desktop::window(*hwnd)?),
        Target::Selector(selector) => resolve_selector(engine, selector),
        Target::Point { point, image } => resolve_point(engine, *point, *image),
    }
}

fn selector_watch(engine: &Engine, selector: &Selector) -> Watch {
    let windows = if selector.has_window_terms() {
        app_windows_matching(engine, selector)
    } else {
        default_window(engine).map(|window| vec![window])
    };
    windows.map(|windows| Watch::windows(&windows)).unwrap_or_default()
}

fn resolve_element(engine: &Engine, id: ElementRef) -> Result<Resolved> {
    let element = engine.uia.run(move |uia| uia.element(id))?;
    let window = element_window(engine, id)?;
    engine.guard.check_window(&window)?;
    Ok(Resolved { window, reach: Reach::Element(element) })
}

fn resolve_window(engine: &Engine, window: WindowInfo) -> Result<Resolved> {
    engine.guard.check_window(&window)?;
    Ok(Resolved { window, reach: Reach::Window })
}

fn resolve_selector(engine: &Engine, selector: &Selector) -> Result<Resolved> {
    let candidates = if selector.has_window_terms() {
        app_windows_matching(engine, selector)?
    } else {
        vec![default_window(engine)?]
    };
    if !selector.has_element_terms() {
        let window = candidates.into_iter().next().ok_or_else(|| no_window(selector))?;
        return resolve_window(engine, window);
    }
    let mut last_error = no_window(selector);
    for window in candidates {
        let snapshot = engine.snapshot(&window)?;
        match selector.resolve(&snapshot) {
            Ok(element) => return Ok(Resolved { reach: Reach::Element(element.clone()), window }),
            Err(error) if error.code == ErrorCode::NotFound => last_error = error,
            Err(error) => return Err(error),
        }
    }
    Err(last_error)
}

fn no_window(selector: &Selector) -> Error {
    Error::not_found(format!("no open window matches {selector}"))
}

fn resolve_point(engine: &Engine, point: Point, image: Option<ImageRef>) -> Result<Resolved> {
    let record = image.map(|image| engine.images.lock().record(image).map(|record| (image, record))).transpose()?;
    let screen = match &record {
        Some((image, record)) => record.screen_point(*image, point)?,
        None => point,
    };
    let under = engine.uia.run(move |uia| uia.element_at(screen))?;
    let window = element_window(engine, under.id)?;
    if let Some((_, record)) = &record {
        record.check_hit_window(window.hwnd)?;
    }
    engine.guard.check_window(&window)?;
    Ok(Resolved { window, reach: Reach::Point { point: screen, under } })
}

#[cfg(test)]
mod tests {
    use deckhand_core::Role;

    use super::*;
    use crate::test_fixture::{element, window};

    fn resolved(reach: Reach) -> Resolved {
        Resolved { window: window(5, "app.exe", "App", deckhand_core::Rect::new(0, 0, 100, 100)), reach }
    }

    #[test]
    fn each_reach_exposes_the_right_element() {
        let button = element(1, 1, Role::Button, "OK");
        let tracked = resolved(Reach::Element(button.clone()));
        assert_eq!(tracked.tracked_element().map(|found| found.id), Some(button.id));
        assert_eq!(tracked.require_element("toggle").unwrap().id, button.id);
        let at_point = resolved(Reach::Point { point: Point::new(3, 4), under: button.clone() });
        assert!(at_point.tracked_element().is_none());
        assert_eq!(at_point.element().map(|found| found.id), Some(button.id));
        let whole = resolved(Reach::Window);
        assert!(whole.element().is_none() && whole.tracked_element().is_none());
        let error = whole.require_element("toggle").unwrap_err();
        assert_eq!(error.message, "toggle needs an element, not the window w5");
    }
}
