use std::time::Duration;

use deckhand_core::{
    Element, ElementRef, Error, ErrorCode, ImageRef, Point, Result, Target, WindowInfo, format::window_line,
    selector::Selector,
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

const LISTED_MATCHES: usize = 8;
const NARROW_THE_SELECTOR: &str =
    "add window: or process: terms to say which, or observe that window and use its element ref";

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

trait InWindow {
    fn window(&self) -> &WindowInfo;
}

impl InWindow for WindowInfo {
    fn window(&self) -> &WindowInfo {
        self
    }
}

impl InWindow for Resolved {
    fn window(&self) -> &WindowInfo {
        &self.window
    }
}

impl Resolved {
    pub fn element_or_point_hit(&self) -> Option<&Element> {
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
        self.element_or_point_hit()
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

pub fn ambiguous<'a>(spec: &str, windows: impl ExactSizeIterator<Item = &'a WindowInfo>, hint: &str) -> Error {
    let count = windows.len();
    let mut lines: Vec<String> = windows.take(LISTED_MATCHES).map(window_line).collect();
    if count > LISTED_MATCHES {
        lines.push("…".to_string());
    }
    let listed = lines.join("; ");
    Error::new(ErrorCode::Ambiguous, format!("{count} windows match {spec}: {listed}; {hint}"))
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
        let window = one_preferred(candidates, selector)?;
        return resolve_window(engine, window);
    }
    one_preferred(element_matches(engine, selector, candidates)?, selector)
}

fn element_matches(engine: &Engine, selector: &Selector, candidates: Vec<WindowInfo>) -> Result<Vec<Resolved>> {
    let mut found = Vec::new();
    let mut last_error = no_window(selector);
    for window in candidates {
        let snapshot = engine.snapshot(&window)?;
        match selector.resolve(&snapshot) {
            Ok(element) => found.push(Resolved { reach: Reach::Element(element.clone()), window }),
            Err(error) if error.code == ErrorCode::NotFound => last_error = error,
            Err(error) => return Err(error),
        }
    }
    if found.is_empty() {
        return Err(last_error);
    }
    Ok(found)
}

fn one_preferred<T: InWindow>(matches: Vec<T>, selector: &Selector) -> Result<T> {
    let mut tied = preferred(matches);
    match tied.len() {
        0 => Err(no_window(selector)),
        1 => Ok(tied.remove(0)),
        _ => Err(ambiguous(&selector.to_string(), tied.iter().map(InWindow::window), NARROW_THE_SELECTOR)),
    }
}

fn preferred<T: InWindow>(matches: Vec<T>) -> Vec<T> {
    let here = kept_if_any(matches, |found| found.window().on_current_desktop);
    kept_if_any(here, |found| found.window().foreground)
}

fn kept_if_any<T>(items: Vec<T>, wanted: impl Fn(&T) -> bool) -> Vec<T> {
    if !items.iter().any(&wanted) {
        return items;
    }
    items.into_iter().filter(|item| wanted(item)).collect()
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
    use deckhand_core::{Rect, Role};

    use super::*;
    use crate::test_fixture::{element, window};

    fn resolved(reach: Reach) -> Resolved {
        Resolved { window: window(5, "app.exe", "App", Rect::new(0, 0, 100, 100)), reach }
    }

    fn placed(hwnd: i64, here: bool, foreground: bool) -> WindowInfo {
        let base = window(hwnd, "app.exe", "App", Rect::new(0, 0, 10, 10));
        WindowInfo { on_current_desktop: here, desktop: (!here).then_some(2), foreground, ..base }
    }

    fn selector(text: &str) -> Selector {
        text.parse().unwrap()
    }

    fn chosen(windows: Vec<WindowInfo>) -> Result<i64> {
        one_preferred(windows, &selector("process:app")).map(|window| window.hwnd.0)
    }

    #[test]
    fn each_reach_exposes_the_right_element() {
        let button = element(1, 1, Role::Button, "OK");
        let tracked = resolved(Reach::Element(button.clone()));
        assert_eq!(tracked.tracked_element().map(|found| found.id), Some(button.id));
        assert_eq!(tracked.require_element("toggle").unwrap().id, button.id);
        let at_point = resolved(Reach::Point { point: Point::new(3, 4), under: button.clone() });
        assert!(at_point.tracked_element().is_none());
        assert_eq!(at_point.element_or_point_hit().map(|found| found.id), Some(button.id));
        let whole = resolved(Reach::Window);
        assert!(whole.element_or_point_hit().is_none() && whole.tracked_element().is_none());
        let error = whole.require_element("toggle").unwrap_err();
        assert_eq!(error.message, "toggle needs an element, not the window w5");
    }

    #[test]
    fn a_single_match_wins_wherever_it_is() {
        assert_eq!(chosen(vec![placed(1, false, false)]).unwrap(), 1);
        assert_eq!(chosen(Vec::new()).unwrap_err().code, ErrorCode::NotFound);
    }

    #[test]
    fn the_current_desktop_then_the_foreground_break_ties() {
        assert_eq!(chosen(vec![placed(1, false, false), placed(2, true, false)]).unwrap(), 2);
        assert_eq!(chosen(vec![placed(1, true, false), placed(2, true, true)]).unwrap(), 2);
        assert_eq!(chosen(vec![placed(1, true, true), placed(2, false, true)]).unwrap(), 1);
    }

    #[test]
    fn matches_that_nothing_tells_apart_are_ambiguous_and_listed_topmost_first() {
        let error = chosen(vec![placed(1, false, false), placed(3, true, false), placed(2, true, false)]).unwrap_err();
        assert_eq!(error.code, ErrorCode::Ambiguous);
        assert!(error.message.starts_with("2 windows match process:app: w3 app.exe"), "{}", error.message);
        assert!(error.message.contains("; w2 app.exe"), "{}", error.message);
        assert!(!error.message.contains("w1 "), "{}", error.message);
        assert!(error.message.ends_with(NARROW_THE_SELECTOR), "{}", error.message);
    }

    #[test]
    fn long_ambiguity_lists_are_capped() {
        let many: Vec<WindowInfo> = (1..=10).map(|hwnd| window(hwnd, "app.exe", "A", Rect::default())).collect();
        let message = ambiguous("A", many.iter(), "pass one window ref").message;
        assert!(message.starts_with("10 windows match A: w1 "), "{message}");
        assert!(message.contains("w8 app.exe \"A\"; …; pass one window ref"), "{message}");
        assert!(!message.contains("w9 "));
    }
}
