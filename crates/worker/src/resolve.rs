mod point;
mod selector;

use std::time::{Duration, Instant};

use winpilot_core::{
    Element, ElementRef, Error, ErrorCode, Point, Result, Snapshot, Target, WindowInfo, format::window_line,
    selector::Selector,
};

pub(crate) use self::selector::selector_snapshots;
use self::{point::resolve_point, selector::wait_for_selector};
use crate::{
    app_windows::app_windows,
    call_scope::CallScope,
    default_window::default_window,
    engine::Engine,
    wait::is_gone,
    window_spec::{self, WindowSpec},
};

const LISTED_MATCHES: usize = 8;
const FOUND_DEPTH: u16 = 1;
const SEARCH_TIMEOUT: Duration = Duration::from_secs(2);
const SEARCH_FLOOR: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Cover {
    Refused,
    Ignored,
}

#[derive(Debug, Clone)]
pub(crate) enum Reach {
    Element(Element),
    Point { point: Point, under: Element },
    Window,
}

#[derive(Debug, Clone)]
pub(crate) struct Resolved {
    pub(crate) window: WindowInfo,
    pub(crate) reach: Reach,
}

impl Resolved {
    pub(crate) fn element_or_point_hit(&self) -> Option<&Element> {
        match &self.reach {
            Reach::Element(element) | Reach::Point { under: element, .. } => Some(element),
            Reach::Window => None,
        }
    }

    pub(crate) fn tracked_element(&self) -> Option<&Element> {
        match &self.reach {
            Reach::Element(element) => Some(element),
            Reach::Point { .. } | Reach::Window => None,
        }
    }

    pub(crate) fn require_element(&self, step: &str) -> Result<&Element> {
        self.element_or_point_hit()
            .ok_or_else(|| Error::invalid(format!("{step} needs an element, not the window {}", self.window.hwnd)))
    }
}

pub(crate) fn window(engine: &Engine, spec: Option<&str>) -> Result<WindowInfo> {
    let Some(spec) = spec.map(str::trim).filter(|spec| !spec.is_empty()) else { return default_window(engine) };
    let window = match window_spec::parse(spec)? {
        WindowSpec::Handle(hwnd) => winpilot_desktop::window(hwnd)?,
        WindowSpec::Element(id) => element_window(engine, id)?,
        spec => first_matching(engine, &spec)?,
    };
    engine.guard.check_window(&window)?;
    Ok(window)
}

pub(crate) fn windows_matching(engine: &Engine, spec: &str) -> Result<Vec<WindowInfo>> {
    match window_spec::parse(spec)? {
        WindowSpec::Handle(hwnd) => Ok(vec![winpilot_desktop::window(hwnd)?]),
        WindowSpec::Element(id) => Ok(vec![element_window(engine, id)?]),
        spec => Ok(app_windows(engine)?.into_iter().filter(|window| window_spec::matches(&spec, window)).collect()),
    }
}

pub(crate) fn element_window(engine: &Engine, id: ElementRef) -> Result<WindowInfo> {
    let hwnd = engine.uia.run(move |uia| uia.window_of(id))?;
    winpilot_desktop::window(hwnd)
}

pub(crate) fn ambiguous<'a>(spec: &str, windows: impl ExactSizeIterator<Item = &'a WindowInfo>, hint: &str) -> Error {
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

pub(crate) fn target(engine: &Engine, target: &Target, call: &CallScope) -> Result<Resolved> {
    target_with(engine, target, call, Cover::Refused)
}

pub(crate) fn target_with(engine: &Engine, target: &Target, call: &CallScope, covering: Cover) -> Result<Resolved> {
    match target {
        Target::Element(id) => resolve_element(engine, *id),
        Target::Window(hwnd) => resolve_window(engine, winpilot_desktop::window(*hwnd)?),
        Target::Selector(selector) => wait_for_selector(engine, selector, call),
        Target::Point { point, image } => resolve_point(engine, *point, *image, covering),
    }
}

pub(crate) fn searched(engine: &Engine, snapshot: Snapshot, selector: &Selector, ends: Instant) -> Result<Snapshot> {
    let Some(limit) = search_time(&snapshot, selector, ends) else { return Ok(snapshot) };
    let Some(hint) = selector.search_hint() else { return Ok(snapshot) };
    let window = snapshot.window.hwnd;
    match engine.uia.run(move |uia| uia.search(window, &hint, limit)) {
        Ok(found) => Ok(with_found(snapshot, found)),
        Err(error) if keeps_the_snapshot(&error) => Ok(snapshot),
        Err(error) => Err(error),
    }
}

fn search_time(snapshot: &Snapshot, selector: &Selector, ends: Instant) -> Option<Duration> {
    let limit = SEARCH_TIMEOUT.min(ends.saturating_duration_since(Instant::now()));
    (snapshot.truncated && limit >= SEARCH_FLOOR && selector.find(snapshot).is_empty()).then_some(limit)
}

fn keeps_the_snapshot(error: &Error) -> bool {
    is_gone(error) || matches!(error.code, ErrorCode::TargetNotResponding | ErrorCode::Internal)
}

fn with_found(mut snapshot: Snapshot, found: Vec<Element>) -> Snapshot {
    let unseen: Vec<Element> = found.into_iter().filter(|element| snapshot.get(element.id).is_none()).collect();
    snapshot.elements.extend(unseen.into_iter().map(|element| Element { depth: FOUND_DEPTH, ..element }));
    snapshot
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

#[cfg(test)]
mod tests {
    use winpilot_core::{Rect, Role, WindowRef};

    use super::*;
    use crate::test_fixture::{element, engine, snapshot, window};

    fn resolved(reach: Reach) -> Resolved {
        Resolved { window: window(5, "app.exe", "App", Rect::new(0, 0, 100, 100)), reach }
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

    fn truncated_page() -> Snapshot {
        let page = snapshot("Page", vec![element(1, 0, Role::Window, "Page"), element(2, 1, Role::Link, "Jobs")]);
        Snapshot { truncated: true, ..page }
    }

    fn ends_in(left: Duration) -> Instant {
        Instant::now() + left
    }

    #[test]
    fn only_a_truncated_snapshot_without_a_match_asks_for_a_native_search() {
        let save: Selector = "role:button name:\"Save\"".parse().unwrap();
        let ends = ends_in(Duration::from_secs(600));
        let mut page = truncated_page();
        assert_eq!(search_time(&page, &save, ends), Some(SEARCH_TIMEOUT));
        page.truncated = false;
        assert_eq!(search_time(&page, &save, ends), None);
        page.truncated = true;
        page.elements.push(element(3, 1, Role::Button, "Save"));
        assert_eq!(search_time(&page, &save, ends), None);
    }

    #[test]
    fn the_native_search_gets_only_the_time_the_caller_has_left() {
        let save: Selector = "role:button name:Save".parse().unwrap();
        let page = truncated_page();
        let limit = search_time(&page, &save, ends_in(Duration::from_millis(500))).unwrap();
        assert!(limit > SEARCH_FLOOR && limit <= Duration::from_millis(500), "{limit:?}");
        assert_eq!(search_time(&page, &save, ends_in(SEARCH_FLOOR / 2)), None);
        assert_eq!(search_time(&page, &save, Instant::now()), None);
    }

    #[test]
    fn a_failed_native_search_keeps_the_snapshot() {
        let save: Selector = "role:button name:Save".parse().unwrap();
        let mut page = truncated_page();
        page.window.hwnd = WindowRef(0);
        assert_eq!(searched(engine(), page.clone(), &save, ends_in(Duration::from_secs(600))).unwrap(), page);
    }

    #[test]
    fn a_search_stopped_by_the_user_or_the_caller_is_not_swallowed() {
        for code in [ErrorCode::TargetNotResponding, ErrorCode::Internal, ErrorCode::NotFound, ErrorCode::StaleRef] {
            assert!(keeps_the_snapshot(&Error::new(code, "x")), "{code}");
        }
        for code in [ErrorCode::Cancelled, ErrorCode::StoppedByUser, ErrorCode::Timeout, ErrorCode::Denied] {
            assert!(!keeps_the_snapshot(&Error::new(code, "x")), "{code}");
        }
    }

    #[test]
    fn found_elements_hang_under_the_root_after_the_walk_and_resolve_by_selector() {
        let save: Selector = "role:button name:Save".parse().unwrap();
        let walked = vec![
            element(1, 0, Role::Window, "Page"),
            element(2, 1, Role::Group, "Feed"),
            element(3, 5, Role::Link, "Jobs"),
        ];
        let found = vec![element(3, 0, Role::Link, "Jobs"), element(9, 0, Role::Button, "Save")];
        let merged = with_found(snapshot("Page", walked), found);
        let shape: Vec<(u32, u16)> = merged.elements.iter().map(|element| (element.id.0, element.depth)).collect();
        assert_eq!(shape, [(1, 0), (2, 1), (3, 5), (9, 1)]);
        assert_eq!(save.resolve(&merged).unwrap().id, ElementRef(9));
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
