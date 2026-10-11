use std::{mem, time::Instant};

use deckhand_core::{Element, ElementRef, Error, ErrorCode, Point, Result, WindowRef};
use windows::Win32::{Foundation::POINT, UI::Accessibility::IUIAutomationElement};
use windows_core::BOOL;

use crate::{
    automation::Uia,
    failure::{UiaResult, is_gone, is_null_result, optional, uia_error},
    properties::NodeProps,
    registry::{Missing, Sighting},
    resolved::Resolved,
    win,
};

pub(crate) const MAX_ANCESTRY: usize = 64;

impl Uia {
    pub fn element(&mut self, id: ElementRef) -> Result<Element> {
        self.resolve(id).map(|resolved| resolved.info)
    }

    pub fn element_at(&mut self, point: Point) -> Result<Element> {
        let found = unsafe {
            self.automation.ElementFromPointBuildCache(POINT { x: point.x, y: point.y }, &self.requests.single)
        }
        .uia(&format!("finding the element at {},{}", point.x, point.y))?;
        Ok(self.adopt(found, || win::window_at(point)))
    }

    pub fn focused(&mut self) -> Result<Option<Element>> {
        match unsafe { self.automation.GetFocusedElementBuildCache(&self.requests.single) } {
            Ok(found) => Ok(Some(self.adopt(found, win::foreground_window))),
            Err(error) if is_null_result(&error) || is_gone(&error) => Ok(None),
            Err(error) => Err(uia_error("reading the focused element", &error)),
        }
    }

    pub fn contains(&mut self, ancestor: ElementRef, descendant: ElementRef) -> Result<bool> {
        let outer = self.resolve(ancestor)?.element;
        let mut current = self.resolve(descendant)?.element;
        for _ in 0..=MAX_ANCESTRY {
            if self.same_element(&outer, &current)? {
                return Ok(true);
            }
            let Some(parent) = self.parent_of(&current)? else { return Ok(false) };
            current = parent;
        }
        Ok(false)
    }

    pub fn window_of(&mut self, id: ElementRef) -> Result<WindowRef> {
        self.registry
            .resolve(id, Instant::now())
            .map(|known| known.fingerprint.window)
            .map_err(|missing| missing_ref(id, missing))
    }

    pub(crate) fn resolve(&mut self, id: ElementRef) -> Result<Resolved> {
        let known = self.registry.resolve(id, Instant::now()).map_err(|missing| missing_ref(id, missing))?;
        match self.refresh(&known.live) {
            Ok(props) => {
                let fingerprint = props.fingerprint(known.fingerprint.window, known.fingerprint.path.clone());
                self.registry.update_fingerprint(id, fingerprint);
                Ok(Resolved { info: props.into_element(id, known.fingerprint.depth()), element: known.live })
            }
            Err(error) if is_gone(&error) => self.refind(id, &known.fingerprint),
            Err(error) => Err(uia_error(&format!("reading {id}"), &error)),
        }
    }

    pub(crate) fn parent_of(&self, element: &IUIAutomationElement) -> Result<Option<IUIAutomationElement>> {
        optional(unsafe { self.walker.GetParentElement(element) }).uia("walking up to a parent element")
    }

    fn refresh(&self, live: &IUIAutomationElement) -> windows_core::Result<NodeProps> {
        let updated = unsafe { live.BuildUpdatedCache(&self.requests.single) }?;
        Ok(NodeProps::read(&updated))
    }

    fn same_element(&self, first: &IUIAutomationElement, second: &IUIAutomationElement) -> Result<bool> {
        unsafe { self.automation.CompareElements(first, second) }.uia("comparing two elements").map(BOOL::as_bool)
    }

    fn adopt(&mut self, found: IUIAutomationElement, fallback: impl FnOnce() -> Option<WindowRef>) -> Element {
        let mut props = NodeProps::read(&found);
        let runtime_id = mem::take(&mut props.runtime_id);
        let id = self.lookup_ref(found, runtime_id, &props, fallback);
        let depth = self.registry.depth(id);
        self.registry.enforce_cap();
        props.into_element(id, depth)
    }

    fn lookup_ref(
        &mut self,
        found: IUIAutomationElement,
        runtime_id: Vec<i32>,
        props: &NodeProps,
        fallback: impl FnOnce() -> Option<WindowRef>,
    ) -> ElementRef {
        let now = Instant::now();
        if let Some(id) = self.registry.revisit(&runtime_id, &found, now) {
            return id;
        }
        let window = self.owning_window(&found, props.native_window).or_else(fallback).unwrap_or(WindowRef(0));
        let sighting = Sighting { live: found, runtime_id, fingerprint: props.fingerprint(window, Vec::new()) };
        self.registry.insert_lookup(sighting, now)
    }

    fn owning_window(&self, element: &IUIAutomationElement, own_handle: WindowRef) -> Option<WindowRef> {
        if let Some(root) = win::root_of(own_handle) {
            return Some(root);
        }
        let mut current = element.clone();
        for _ in 0..MAX_ANCESTRY {
            let parent = self.parent_with_handle(&current)?;
            let handle = unsafe { parent.CachedNativeWindowHandle() }.map_or(WindowRef(0), win::window_ref);
            if let Some(root) = win::root_of(handle) {
                return Some(root);
            }
            current = parent;
        }
        None
    }

    fn parent_with_handle(&self, element: &IUIAutomationElement) -> Option<IUIAutomationElement> {
        let parent = unsafe { self.walker.GetParentElementBuildCache(element, &self.requests.native_window) };
        optional(parent).ok().flatten()
    }
}

pub(crate) fn missing_ref(id: ElementRef, missing: Missing) -> Error {
    match missing {
        Missing::NeverIssued => {
            Error::new(ErrorCode::StaleRef, format!("{id} is not known (the worker may have restarted); observe again"))
        }
        Missing::Gone => stale_ref(id),
    }
}

pub(crate) fn stale_ref(id: ElementRef) -> Error {
    Error::new(ErrorCode::StaleRef, format!("{id} no longer exists; observe again"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        SnapshotOptions,
        test_support::{handle, taskbar},
    };

    #[test]
    fn unknown_refs_are_stale() {
        let error = handle().run(|uia| uia.element(ElementRef(999_999))).unwrap_err();
        assert_eq!(error.code, ErrorCode::StaleRef);
        assert_eq!(error.message, "e999999 is not known (the worker may have restarted); observe again");
        let window = handle().run(|uia| uia.window_of(ElementRef(5))).unwrap_err();
        assert_eq!(window.code, ErrorCode::StaleRef);
    }

    #[test]
    fn element_round_trips_the_root() {
        let window = taskbar();
        let (root, again) = handle()
            .run(move |uia| {
                let snapshot = uia.snapshot(&window, &SnapshotOptions::default())?;
                let root = snapshot.elements[0].clone();
                let again = uia.element(root.id)?;
                Ok((root, again))
            })
            .unwrap();
        assert_eq!(again.id, root.id);
        assert_eq!(again.role, root.role);
        assert_eq!(again.class_name, root.class_name);
        assert_eq!(again.automation_id, root.automation_id);
        assert_eq!(again.depth, 0);
    }

    #[test]
    fn window_of_returns_the_snapshot_window() {
        let window = taskbar();
        let hwnd = window.hwnd;
        let found = handle()
            .run(move |uia| {
                let snapshot = uia.snapshot(&window, &SnapshotOptions::default())?;
                let last = snapshot.elements.last().map_or(snapshot.elements[0].id, |element| element.id);
                uia.window_of(last)
            })
            .unwrap();
        assert_eq!(found, hwnd);
    }

    #[test]
    fn contains_follows_the_tree() {
        let window = taskbar();
        let (inside, self_contained, reversed) = handle()
            .run(move |uia| {
                let snapshot = uia.snapshot(&window, &SnapshotOptions::default())?;
                let root = snapshot.elements[0].id;
                let Some(child) = snapshot.elements.iter().find(|element| element.depth == 1) else {
                    return Ok((true, true, false));
                };
                Ok((uia.contains(root, child.id)?, uia.contains(root, root)?, uia.contains(child.id, root)?))
            })
            .unwrap();
        assert!(inside);
        assert!(self_contained);
        assert!(!reversed);
    }

    #[test]
    fn element_at_the_taskbar_centre() {
        let window = taskbar();
        let hwnd = window.hwnd;
        let (hit, owner, inside, under_point) = handle()
            .run(move |uia| {
                let snapshot = uia.snapshot(&window, &SnapshotOptions::default())?;
                let root = &snapshot.elements[0];
                let centre = root.rect.center();
                let under_point = win::window_at(centre);
                let started = Instant::now();
                let hit = uia.element_at(centre)?;
                eprintln!("element_at: {:?}", started.elapsed());
                let owner = uia.window_of(hit.id)?;
                let inside = uia.contains(root.id, hit.id)?;
                Ok((hit, owner, inside, under_point))
            })
            .unwrap();
        assert!(hit.id.0 > 0);
        if under_point == Some(hwnd) {
            assert_eq!(owner, hwnd);
            assert!(inside);
        }
    }

    #[test]
    fn element_at_without_a_snapshot_finds_its_window() {
        let window = taskbar();
        let hwnd = window.hwnd;
        let probe = handle()
            .run(move |uia| Ok(uia.snapshot(&window, &SnapshotOptions::default())?.elements[0].rect.center()))
            .unwrap();
        let (timings, under_point) = handle()
            .run(move |uia| {
                let mut timings = Vec::new();
                for _ in 0..5 {
                    let started = Instant::now();
                    let hit = uia.element_at(probe)?;
                    timings.push((started.elapsed(), uia.window_of(hit.id)?));
                }
                Ok((timings, win::window_at(probe)))
            })
            .unwrap();
        for (elapsed, owner) in timings {
            eprintln!("element_at without a snapshot: {elapsed:?}");
            if under_point == Some(hwnd) {
                assert_eq!(owner, hwnd);
            }
        }
    }

    #[test]
    fn focused_element_belongs_to_a_real_window() {
        let owner = handle()
            .run(|uia| {
                let Some(element) = uia.focused()? else { return Ok(None) };
                uia.window_of(element.id).map(Some)
            })
            .unwrap();
        if let Some(window) = owner {
            assert!(window.0 == 0 || win::window_exists(window));
        }
    }

    #[test]
    fn element_at_reuses_snapshot_refs() {
        let window = taskbar();
        let (root, hit) = handle()
            .run(move |uia| {
                let snapshot = uia.snapshot(&window, &SnapshotOptions::default())?;
                let root = snapshot.elements[0].clone();
                let corner = Point::new(root.rect.x + 1, root.rect.y + 1);
                Ok((root, uia.element_at(corner)?))
            })
            .unwrap();
        if hit.class_name == root.class_name && hit.role == root.role {
            assert_eq!(hit.id, root.id);
        }
    }

    #[test]
    fn focused_element_reads_without_error() {
        let focused = handle().run(Uia::focused).unwrap();
        if let Some(element) = focused {
            assert!(element.id.0 > 0);
        }
    }
}
