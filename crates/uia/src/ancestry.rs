use deckhand_core::{ElementRef, Result, WindowRef};
use windows::Win32::UI::Accessibility::IUIAutomationElement;
use windows_core::BOOL;

use crate::{
    automation::Uia,
    failure::{UiaResult, optional},
    win,
};

pub(crate) const MAX_ANCESTRY: usize = 64;

impl Uia {
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

    pub(crate) fn parent_of(&self, element: &IUIAutomationElement) -> Result<Option<IUIAutomationElement>> {
        optional(unsafe { self.walker.GetParentElement(element) }).uia("walking up to a parent element")
    }

    pub(crate) fn owning_window(&self, element: &IUIAutomationElement, own_handle: WindowRef) -> Option<WindowRef> {
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
        let parent = unsafe { self.walker.GetParentElementBuildCache(element, &self.requests.element_hwnd) };
        optional(parent).ok().flatten()
    }

    fn same_element(&self, first: &IUIAutomationElement, second: &IUIAutomationElement) -> Result<bool> {
        unsafe { self.automation.CompareElements(first, second) }.uia("comparing two elements").map(BOOL::as_bool)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        SnapshotOptions,
        test_support::{first_element, handle, taskbar},
    };

    #[test]
    fn contains_follows_the_tree() {
        let window = taskbar();
        let (inside, self_contained, reversed) = handle()
            .run(move |uia| {
                let snapshot = uia.snapshot(&window, &SnapshotOptions::default())?;
                let root = first_element(&snapshot)?.id;
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
}
