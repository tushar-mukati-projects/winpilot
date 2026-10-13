use windows::Win32::UI::Accessibility::IUIAutomationElement;
use windows_core::BOOL;
use winpilot_core::{ElementRef, Result, WindowRef};

use crate::{
    automation::Uia,
    failure::{UiaResult, optional},
    properties::{cached_runtime_id, cached_text, current_runtime_id},
    tree::cached_children,
    win,
};

pub(crate) const MAX_ANCESTRY: usize = 64;

impl Uia {
    pub fn contains(&mut self, ancestor: ElementRef, descendant: ElementRef) -> Result<bool> {
        let outer = self.resolve(ancestor)?.element;
        let mut current = self.resolve(descendant)?.element;
        let mut chain = Vec::new();
        for _ in 0..=MAX_ANCESTRY {
            if self.same_element(&outer, &current)? {
                return Ok(true);
            }
            let parent = self.parent_of(&current)?;
            chain.push(current);
            let Some(parent) = parent else { break };
            current = parent;
        }
        Ok(self.hosts_one_of(&outer, &chain))
    }

    pub fn marked_ancestor(&mut self, element: ElementRef, marked: impl Fn(&str) -> bool) -> Result<Option<String>> {
        let mut current = self.resolve(element)?.element;
        for _ in 0..=MAX_ANCESTRY {
            let automation_id = cached_text(unsafe { current.CurrentAutomationId() });
            if marked(&automation_id) {
                return Ok(Some(automation_id));
            }
            let Some(parent) = self.parent_of(&current)? else { break };
            current = parent;
        }
        Ok(None)
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

    pub(crate) fn parent_with_handle(&self, element: &IUIAutomationElement) -> Option<IUIAutomationElement> {
        let parent = unsafe { self.walker.GetParentElementBuildCache(element, &self.requests.element_hwnd) };
        optional(parent).ok().flatten()
    }

    fn hosts_one_of(&self, host: &IUIAutomationElement, chain: &[IUIAutomationElement]) -> bool {
        let hosted = self.child_runtime_ids(host);
        if hosted.is_empty() {
            return false;
        }
        let chain: Vec<Vec<i32>> = chain.iter().map(current_runtime_id).collect();
        leads_down_through(&hosted, &chain, || self.runtime_ids_up_from(host))
    }

    fn runtime_ids_up_from(&self, element: &IUIAutomationElement) -> Vec<Vec<i32>> {
        let mut ids = Vec::new();
        let mut current = element.clone();
        for _ in 0..=MAX_ANCESTRY {
            ids.push(current_runtime_id(&current));
            let Ok(Some(parent)) = self.parent_of(&current) else { break };
            current = parent;
        }
        ids
    }

    fn child_runtime_ids(&self, element: &IUIAutomationElement) -> Vec<Vec<i32>> {
        let Ok(listed) = (unsafe { element.BuildUpdatedCache(&self.requests.children_ids) }) else { return Vec::new() };
        cached_children(&listed).iter().map(cached_runtime_id).collect()
    }

    fn same_element(&self, first: &IUIAutomationElement, second: &IUIAutomationElement) -> Result<bool> {
        unsafe { self.automation.CompareElements(first, second) }.uia("comparing two elements").map(BOOL::as_bool)
    }
}

fn leads_down_through(
    hosted: &[Vec<i32>],
    chain: &[Vec<i32>],
    host_and_ancestors: impl FnOnce() -> Vec<Vec<i32>>,
) -> bool {
    let shared: Vec<&Vec<i32>> = hosted.iter().filter(|child| !child.is_empty() && chain.contains(child)).collect();
    if shared.is_empty() {
        return false;
    }
    let above = host_and_ancestors();
    shared.into_iter().any(|child| !above.contains(child))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        SnapshotOptions,
        test_support::{first_element, handle, taskbar},
    };

    fn id(value: i32) -> Vec<i32> {
        vec![42, value]
    }

    fn nothing_above() -> Vec<Vec<i32>> {
        Vec::new()
    }

    #[test]
    fn a_host_contains_a_tree_it_lists_that_names_another_parent() {
        let chain = [id(9), id(8), id(7), id(2), id(1)];
        assert!(leads_down_through(&[id(3), id(7)], &chain, nothing_above));
        assert!(leads_down_through(&[id(9)], &chain, nothing_above));
    }

    #[test]
    fn a_host_whose_children_are_not_on_the_chain_contains_nothing() {
        let chain = [id(9), id(8), id(1)];
        assert!(!leads_down_through(&[id(3), id(4)], &chain, nothing_above));
        assert!(!leads_down_through(&[], &chain, nothing_above));
    }

    #[test]
    fn children_that_are_the_hosts_own_ancestors_do_not_count() {
        let chain = [id(9), id(5), id(1)];
        let host_and_ancestors = || vec![id(6), id(5), id(1)];
        assert!(!leads_down_through(&[id(5), id(1)], &chain, host_and_ancestors));
        assert!(leads_down_through(&[id(5), id(9)], &chain, host_and_ancestors));
    }

    #[test]
    fn the_hosts_ancestors_are_only_read_when_a_child_is_on_the_chain() {
        let read_above = || -> Vec<Vec<i32>> { panic!("read the host's ancestors") };
        assert!(!leads_down_through(&[id(3)], &[id(9), id(1)], read_above));
    }

    #[test]
    fn missing_runtime_ids_never_match() {
        assert!(!leads_down_through(&[Vec::new()], &[Vec::new(), id(1)], nothing_above));
    }

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
