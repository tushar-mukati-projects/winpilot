use std::{mem, time::Instant};

use deckhand_core::{Element, ElementRef, Point, Result, WindowRef};
use windows::Win32::{Foundation::POINT, UI::Accessibility::IUIAutomationElement};

use crate::{
    automation::Uia,
    failure::{UiaResult, is_gone, is_null_out_pointer, uia_error},
    properties::NodeProps,
    registry::Sighting,
    win,
    window_identity::WindowIdentity,
};

impl Uia {
    pub fn element_at(&mut self, point: Point) -> Result<Element> {
        let found = unsafe {
            self.automation.ElementFromPointBuildCache(POINT { x: point.x, y: point.y }, &self.requests.element_props)
        }
        .uia(&format!("finding the element at {},{}", point.x, point.y))?;
        Ok(self.adopt(found, || win::window_at(point)))
    }

    pub fn focused(&mut self) -> Result<Option<Element>> {
        match unsafe { self.automation.GetFocusedElementBuildCache(&self.requests.element_props) } {
            Ok(found) => Ok(Some(self.adopt(found, win::foreground_window))),
            Err(error) if is_null_out_pointer(&error) || is_gone(&error) => Ok(None),
            Err(error) => Err(uia_error("reading the focused element", &error)),
        }
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
        let window = self.owning_window(&found, props.native_window).or_else(fallback);
        let identity = window.and_then(WindowIdentity::read).unwrap_or_else(WindowIdentity::unknown);
        let sighting = Sighting { live: found, runtime_id, fingerprint: props.fingerprint(identity, None) };
        self.registry.insert_lookup(sighting, now)
    }
}

#[cfg(test)]
mod tests {
    use deckhand_core::ErrorCode;

    use super::*;
    use crate::{
        SnapshotOptions,
        test_support::{first_element, handle, taskbar},
    };

    #[test]
    fn element_at_the_taskbar_centre() {
        let window = taskbar();
        let hwnd = window.hwnd;
        let (hit, owner, inside, under_point) = handle()
            .run(move |uia| {
                let snapshot = uia.snapshot(&window, &SnapshotOptions::default())?;
                let root = first_element(&snapshot)?;
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
            .run(move |uia| Ok(first_element(&uia.snapshot(&window, &SnapshotOptions::default())?)?.rect.center()))
            .unwrap();
        let (timings, under_point) = handle()
            .run(move |uia| {
                let mut timings = Vec::new();
                for _ in 0..5 {
                    let started = Instant::now();
                    let hit = uia.element_at(probe)?;
                    timings.push((started.elapsed(), uia.window_of(hit.id)?, hit.depth));
                }
                Ok((timings, win::window_at(probe)))
            })
            .unwrap();
        for (elapsed, owner, depth) in timings {
            eprintln!("element_at without a snapshot: {elapsed:?}");
            assert_eq!(depth, 0);
            if under_point == Some(hwnd) {
                assert_eq!(owner, hwnd);
            }
        }
    }

    #[test]
    fn focused_element_belongs_to_a_real_window_or_none() {
        let owner = handle()
            .run(|uia| {
                let Some(element) = uia.focused()? else { return Ok(None) };
                match uia.window_of(element.id) {
                    Ok(window) => Ok(Some(window)),
                    Err(error) if error.code == ErrorCode::NotFound => Ok(None),
                    Err(error) => Err(error),
                }
            })
            .unwrap();
        if let Some(window) = owner {
            assert!(win::window_exists(window));
        }
    }

    #[test]
    fn element_at_reuses_snapshot_refs() {
        let window = taskbar();
        let (root, hit) = handle()
            .run(move |uia| {
                let snapshot = uia.snapshot(&window, &SnapshotOptions::default())?;
                let root = first_element(&snapshot)?.clone();
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
