use windows::Win32::{
    Foundation::HWND,
    UI::{
        Accessibility::{IUIAutomationCacheRequest, IUIAutomationElement},
        WindowsAndMessaging::{GA_PARENT, GetAncestor, GetDesktopWindow},
    },
};
use windows_core::BOOL;

use crate::{
    automation::Uia,
    failure::{Failure, classify},
    tree::cached_children,
    win,
    win32_combo::dropdown_list,
};

const DROPDOWN_LIST_CLASS: &str = "ComboLBox";

#[derive(Clone, Copy)]
pub(crate) struct TreeRequests<'a> {
    pub(crate) whole: &'a IUIAutomationCacheRequest,
    pub(crate) level: &'a IUIAutomationCacheRequest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reach {
    Whole,
    Level,
    Unread,
    Leaf,
}

pub(crate) struct CachedNode {
    pub(crate) element: IUIAutomationElement,
    reach: Reach,
}

impl CachedNode {
    fn new(element: IUIAutomationElement, reach: Reach) -> Self {
        Self { element, reach }
    }
}

pub(crate) fn read_tree(root: &IUIAutomationElement, requests: TreeRequests<'_>) -> windows_core::Result<CachedNode> {
    match build(root, requests.whole) {
        Ok(whole) => Ok(CachedNode::new(whole, Reach::Whole)),
        Err(error) if is_broken_tree(&error) => {
            build(root, requests.level).map(|level| CachedNode::new(level, Reach::Level)).map_err(|_| error)
        }
        Err(error) => Err(error),
    }
}

impl Uia {
    pub(crate) fn tree_children(&self, node: &CachedNode, requests: TreeRequests<'_>) -> Vec<CachedNode> {
        match node.reach {
            Reach::Whole => nodes(cached_children(&node.element), Reach::Whole),
            Reach::Level => nodes(self.children_that_belong(&node.element), Reach::Unread),
            Reach::Unread => self.tree_children(&read_further(&node.element, requests), requests),
            Reach::Leaf => Vec::new(),
        }
    }

    fn children_that_belong(&self, parent: &IUIAutomationElement) -> Vec<IUIAutomationElement> {
        cached_children(parent).into_iter().filter(|child| !self.strays_from(child, parent)).collect()
    }

    fn strays_from(&self, child: &IUIAutomationElement, parent: &IUIAutomationElement) -> bool {
        let Some(window) = top_level_window_of(child) else { return false };
        !is_dropdown_of(parent, window) && !self.reports_as_parent(child, parent)
    }

    fn reports_as_parent(&self, child: &IUIAutomationElement, parent: &IUIAutomationElement) -> bool {
        let Ok(reported) = (unsafe { self.walker.GetParentElement(child) }) else { return false };
        unsafe { self.automation.CompareElements(&reported, parent) }.is_ok_and(BOOL::as_bool)
    }
}

fn read_further(element: &IUIAutomationElement, requests: TreeRequests<'_>) -> CachedNode {
    match build(element, requests.whole) {
        Ok(whole) => CachedNode::new(whole, Reach::Whole),
        Err(error) if is_broken_tree(&error) => match build(element, requests.level) {
            Ok(level) => CachedNode::new(level, Reach::Level),
            Err(_) => CachedNode::new(element.clone(), Reach::Leaf),
        },
        Err(_) => CachedNode::new(element.clone(), Reach::Leaf),
    }
}

fn build(
    element: &IUIAutomationElement,
    request: &IUIAutomationCacheRequest,
) -> windows_core::Result<IUIAutomationElement> {
    unsafe { element.BuildUpdatedCache(request) }
}

fn is_broken_tree(error: &windows_core::Error) -> bool {
    classify(error.code()) == Failure::Other
}

fn nodes(elements: Vec<IUIAutomationElement>, reach: Reach) -> Vec<CachedNode> {
    elements.into_iter().map(|element| CachedNode::new(element, reach)).collect()
}

fn top_level_window_of(element: &IUIAutomationElement) -> Option<HWND> {
    let window = unsafe { element.CachedNativeWindowHandle() }.ok().filter(|window| !window.is_invalid())?;
    (unsafe { GetAncestor(window, GA_PARENT) } == unsafe { GetDesktopWindow() }).then_some(window)
}

fn is_dropdown_of(parent: &IUIAutomationElement, window: HWND) -> bool {
    if win::class_name(win::window_ref(window)).as_deref() != Some(DROPDOWN_LIST_CLASS) {
        return false;
    }
    let Some(combo) = unsafe { parent.CachedNativeWindowHandle() }.ok().filter(|combo| !combo.is_invalid()) else {
        return false;
    };
    dropdown_list(combo) == Some(window)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        failure::UiaResult,
        test_support::{handle, taskbar},
        tree_path::View,
    };

    #[test]
    fn only_unexpected_failures_count_as_a_broken_tree() {
        let failure = |code: u32| windows_core::Error::from_hresult(windows_core::HRESULT(code as i32));
        assert!(is_broken_tree(&failure(0x8000_4005)));
        assert!(!is_broken_tree(&failure(0x8004_0201)));
        assert!(!is_broken_tree(&failure(0x8013_1505)));
        assert!(!is_broken_tree(&failure(0x8007_0005)));
    }

    #[test]
    fn a_level_read_lists_the_same_children_as_a_whole_read() {
        let window = taskbar().hwnd;
        let (whole, level) = handle()
            .run(move |uia| {
                let root = unsafe { uia.automation.ElementFromHandle(win::hwnd(window)) }.uia("opening the taskbar")?;
                let requests = uia.requests.props_in(View::Full);
                let whole =
                    CachedNode::new(build(&root, requests.whole).uia("reading the whole taskbar")?, Reach::Whole);
                let level =
                    CachedNode::new(build(&root, requests.level).uia("reading the taskbar's top level")?, Reach::Level);
                let count = |node: &CachedNode| count_nodes(uia, node, requests);
                Ok((count(&whole), count(&level)))
            })
            .unwrap();
        assert_eq!(whole, level);
    }

    fn count_nodes(uia: &Uia, node: &CachedNode, requests: TreeRequests<'_>) -> usize {
        1 + uia.tree_children(node, requests).iter().map(|child| count_nodes(uia, child, requests)).sum::<usize>()
    }
}
