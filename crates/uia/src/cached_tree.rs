use std::collections::HashSet;

use windows::Win32::{
    Foundation::HWND,
    UI::{
        Accessibility::{IUIAutomationCacheRequest, IUIAutomationElement, IUIAutomationElementArray},
        WindowsAndMessaging::{GA_PARENT, GetAncestor, GetDesktopWindow},
    },
};
use winpilot_core::WindowRef;
use winpilot_win32::{class_name, hwnd_of, ref_of};

use crate::{
    automation::Uia,
    failure::{is_gone, is_not_responding},
    properties::cached_runtime_id,
    requests::TreeRequests,
    win32_combo::dropdown_list,
};

const DROPDOWN_LIST_CLASS: &str = "ComboLBox";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reach {
    Whole,
    Level,
    Unread,
    Leaf,
}

pub(crate) struct CachedNode<E = IUIAutomationElement> {
    pub(crate) element: E,
    reach: Reach,
}

impl<E> CachedNode<E> {
    fn new(element: E, reach: Reach) -> Self {
        Self { element, reach }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NativeWindow {
    Missing,
    Child,
    TopLevel(WindowRef),
}

#[derive(Debug, Default)]
pub(crate) struct TreeRead {
    windows: Vec<WindowRef>,
    listed: HashSet<Vec<i32>>,
    pub(crate) incomplete: bool,
}

impl TreeRead {
    fn starting_at(home: Option<WindowRef>, root_runtime_id: Vec<i32>) -> Self {
        let mut read = Self { windows: home.into_iter().collect(), listed: HashSet::new(), incomplete: false };
        read.first_sighting(root_runtime_id);
        read
    }

    fn first_sighting(&mut self, runtime_id: Vec<i32>) -> bool {
        runtime_id.is_empty() || self.listed.insert(runtime_id)
    }

    fn adopt(&mut self, window: WindowRef) {
        if !self.windows.contains(&window) {
            self.windows.push(window);
        }
    }

    fn or_leaf<E: Clone>(&mut self, node: windows::core::Result<CachedNode<E>>, element: &E) -> CachedNode<E> {
        node.unwrap_or_else(|_| {
            self.incomplete = true;
            CachedNode::new(element.clone(), Reach::Leaf)
        })
    }
}

impl Uia {
    pub(crate) fn read_tree(
        &self,
        root: &IUIAutomationElement,
        requests: &TreeRequests,
    ) -> windows::core::Result<(CachedNode, TreeRead)> {
        let node = read_node(root, requests)?;
        let home = self.owning_window(&node.element, own_window(&node.element));
        let read = TreeRead::starting_at(home, cached_runtime_id(&node.element));
        Ok((node, read))
    }

    pub(crate) fn tree_children(
        &self,
        node: &CachedNode,
        requests: &TreeRequests,
        read: &mut TreeRead,
    ) -> Vec<CachedNode> {
        let reach = match node.reach {
            Reach::Whole => Reach::Whole,
            Reach::Level => Reach::Unread,
            Reach::Unread => {
                let further = read.or_leaf(read_node(&node.element, requests), &node.element);
                return self.tree_children(&further, requests, read);
            }
            Reach::Leaf => return Vec::new(),
        };
        let children = new_children(cached_children(&node.element), read, cached_runtime_id, |child, read| {
            self.belongs_to_read(child, &node.element, read)
        });
        children.into_iter().map(|element| CachedNode::new(element, reach)).collect()
    }

    fn belongs_to_read(
        &self,
        child: &IUIAutomationElement,
        parent: &IUIAutomationElement,
        read: &mut TreeRead,
    ) -> bool {
        let window = native_window_of(child);
        let belongs =
            belongs(window, |window| is_dropdown_of(parent, window), || self.placed_inside(child, &read.windows));
        if let (true, NativeWindow::TopLevel(window)) = (belongs, window) {
            read.adopt(window);
        }
        belongs
    }

    fn placed_inside(&self, element: &IUIAutomationElement, windows: &[WindowRef]) -> bool {
        let Some(parent) = self.parent_with_handle(element) else { return false };
        self.owning_window(&parent, own_window(&parent)).is_some_and(|window| windows.contains(&window))
    }
}

fn belongs(
    window: NativeWindow,
    is_dropdown: impl FnOnce(WindowRef) -> bool,
    placed_inside: impl FnOnce() -> bool,
) -> bool {
    match window {
        NativeWindow::Missing | NativeWindow::Child => true,
        NativeWindow::TopLevel(window) => is_dropdown(window) || placed_inside(),
    }
}

fn new_children<E>(
    children: Vec<E>,
    read: &mut TreeRead,
    runtime_id_of: impl Fn(&E) -> Vec<i32>,
    mut belongs_to_read: impl FnMut(&E, &mut TreeRead) -> bool,
) -> Vec<E> {
    children
        .into_iter()
        .filter(|child| belongs_to_read(child, read) && read.first_sighting(runtime_id_of(child)))
        .collect()
}

pub(crate) fn cached_children(element: &IUIAutomationElement) -> Vec<IUIAutomationElement> {
    unsafe { element.GetCachedChildren() }.map(|children| array_elements(&children)).unwrap_or_default()
}

pub(crate) fn array_elements(array: &IUIAutomationElementArray) -> Vec<IUIAutomationElement> {
    let count = unsafe { array.Length() }.unwrap_or(0);
    (0..count).filter_map(|index| unsafe { array.GetElement(index) }.ok()).collect()
}

fn read_node(element: &IUIAutomationElement, requests: &TreeRequests) -> windows::core::Result<CachedNode> {
    whole_or_level(|| build(element, &requests.whole), || build(element, &requests.level))
}

fn whole_or_level<E>(
    whole: impl FnOnce() -> windows::core::Result<E>,
    level: impl FnOnce() -> windows::core::Result<E>,
) -> windows::core::Result<CachedNode<E>> {
    match whole() {
        Ok(read) => Ok(CachedNode::new(read, Reach::Whole)),
        Err(error) if is_gone(&error) || is_not_responding(&error) => Err(error),
        Err(error) => level().map(|read| CachedNode::new(read, Reach::Level)).map_err(|_| error),
    }
}

fn build(
    element: &IUIAutomationElement,
    request: &IUIAutomationCacheRequest,
) -> windows::core::Result<IUIAutomationElement> {
    unsafe { element.BuildUpdatedCache(request) }
}

fn own_window(element: &IUIAutomationElement) -> WindowRef {
    cached_window_handle(element).map_or(WindowRef(0), ref_of)
}

fn cached_window_handle(element: &IUIAutomationElement) -> Option<HWND> {
    unsafe { element.CachedNativeWindowHandle() }.ok().filter(|window| !window.is_invalid())
}

fn native_window_of(element: &IUIAutomationElement) -> NativeWindow {
    let Some(window) = cached_window_handle(element) else { return NativeWindow::Missing };
    if unsafe { GetAncestor(window, GA_PARENT) } == unsafe { GetDesktopWindow() } {
        NativeWindow::TopLevel(ref_of(window))
    } else {
        NativeWindow::Child
    }
}

fn is_dropdown_of(parent: &IUIAutomationElement, window: WindowRef) -> bool {
    if class_name(hwnd_of(window)) != DROPDOWN_LIST_CLASS {
        return false;
    }
    let Some(combo) = cached_window_handle(parent) else { return false };
    dropdown_list(combo).map(ref_of) == Some(window)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use windows::Win32::{
        Foundation::{E_FAIL, E_INVALIDARG, RPC_E_DISCONNECTED},
        UI::Accessibility::{UIA_E_ELEMENTNOTAVAILABLE, UIA_E_TIMEOUT},
    };

    use super::*;
    use crate::{
        failure::UiaResult,
        test_support::{handle, taskbar},
        tree_path::View,
    };

    const HOME: WindowRef = WindowRef(10);
    const DROPDOWN: WindowRef = WindowRef(20);
    const OTHER_APP: WindowRef = WindowRef(30);

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct Fake {
        runtime_id: Vec<i32>,
        inside: bool,
    }

    fn fake(id: i32, inside: bool) -> Fake {
        Fake { runtime_id: vec![42, id], inside }
    }

    fn listed(children: Vec<Fake>, read: &mut TreeRead) -> Vec<i32> {
        new_children(children, read, |child| child.runtime_id.clone(), |child, _| child.inside)
            .into_iter()
            .map(|child| child.runtime_id[1])
            .collect()
    }

    fn failure(code: u32) -> windows::core::Error {
        windows::core::Error::from_hresult(windows::core::HRESULT(code as i32))
    }

    fn failed_with(node: &windows::core::Result<CachedNode<&str>>) -> Option<windows::core::HRESULT> {
        node.as_ref().map_err(windows::core::Error::code).err()
    }

    #[test]
    fn a_whole_read_that_fails_otherwise_keeps_the_children_through_a_level_read() {
        for code in [E_FAIL.0 as u32, E_INVALIDARG.0 as u32] {
            let node = whole_or_level(|| Err(failure(code)), || Ok("level")).unwrap();
            assert_eq!((node.element, node.reach), ("level", Reach::Level), "{code:#x}");
        }
        let node = whole_or_level(|| Ok("whole"), || panic!("read the level after a whole read")).unwrap();
        assert_eq!((node.element, node.reach), ("whole", Reach::Whole));
    }

    #[test]
    fn a_timed_out_or_gone_node_skips_the_level_read_and_marks_the_read_incomplete() {
        for code in [UIA_E_TIMEOUT, UIA_E_ELEMENTNOTAVAILABLE, RPC_E_DISCONNECTED.0 as u32] {
            let mut read = TreeRead::starting_at(Some(HOME), Vec::new());
            let unanswered = whole_or_level(|| Err(failure(code)), || panic!("read the level after {code:#x}"));
            assert_eq!(failed_with(&unanswered), Some(failure(code).code()));
            let leaf = read.or_leaf(unanswered, &"node");
            assert_eq!((leaf.element, leaf.reach, read.incomplete), ("node", Reach::Leaf, true), "{code:#x}");
        }
    }

    #[test]
    fn a_node_neither_read_can_reach_stays_a_leaf_and_keeps_the_first_error() {
        let mut read = TreeRead::starting_at(Some(HOME), Vec::new());
        let level = read.or_leaf(whole_or_level(|| Err(failure(E_FAIL.0 as u32)), || Ok("level")), &"node");
        assert_eq!((level.element, level.reach, read.incomplete), ("level", Reach::Level, false));
        let lost = whole_or_level(|| Err(failure(E_FAIL.0 as u32)), || Err(failure(E_INVALIDARG.0 as u32)));
        assert_eq!(failed_with(&lost), Some(E_FAIL));
        let leaf = read.or_leaf(lost, &"node");
        assert_eq!((leaf.element, leaf.reach, read.incomplete), ("node", Reach::Leaf, true));
    }

    #[test]
    fn elements_without_a_top_level_window_always_belong() {
        let never = |_: WindowRef| -> bool { panic!("asked about a dropdown") };
        assert!(belongs(NativeWindow::Missing, never, || panic!("asked where it is placed")));
        assert!(belongs(NativeWindow::Child, never, || panic!("asked where it is placed")));
    }

    #[test]
    fn top_level_windows_belong_only_as_the_dropdown_or_when_placed_inside() {
        assert!(belongs(NativeWindow::TopLevel(DROPDOWN), |window| window == DROPDOWN, || false));
        assert!(belongs(NativeWindow::TopLevel(OTHER_APP), |_| false, || true));
        assert!(!belongs(NativeWindow::TopLevel(OTHER_APP), |_| false, || false));
    }

    #[test]
    fn the_dropdown_is_kept_without_asking_where_it_is_placed() {
        let asked = Cell::new(false);
        let kept = belongs(
            NativeWindow::TopLevel(DROPDOWN),
            |_| true,
            || {
                asked.set(true);
                false
            },
        );
        assert!(kept);
        assert!(!asked.get());
    }

    #[test]
    fn a_read_starts_inside_its_own_window_with_its_root_listed() {
        let mut read = TreeRead::starting_at(Some(HOME), vec![42, 1]);
        assert_eq!(read.windows, [HOME]);
        assert!(!read.first_sighting(vec![42, 1]));
        assert!(read.first_sighting(vec![42, 2]));
        assert!(TreeRead::starting_at(None, Vec::new()).windows.is_empty());
    }

    #[test]
    fn windows_placed_inside_are_adopted_once() {
        let mut read = TreeRead::starting_at(Some(HOME), Vec::new());
        read.adopt(OTHER_APP);
        read.adopt(OTHER_APP);
        read.adopt(HOME);
        assert_eq!(read.windows, [HOME, OTHER_APP]);
    }

    #[test]
    fn an_element_is_listed_once_however_often_the_tree_reaches_it() {
        let mut read = TreeRead::starting_at(Some(HOME), vec![42, 1]);
        assert_eq!(listed(vec![fake(2, true), fake(3, true), fake(2, true)], &mut read), [2, 3]);
        assert_eq!(listed(vec![fake(4, true), fake(3, true)], &mut read), [4]);
        assert_eq!(listed(vec![fake(1, true)], &mut read), Vec::<i32>::new());
    }

    #[test]
    fn elements_of_other_windows_are_dropped_and_do_not_hide_later_copies() {
        let mut read = TreeRead::starting_at(Some(HOME), Vec::new());
        assert_eq!(listed(vec![fake(5, false), fake(6, true)], &mut read), [6]);
        assert_eq!(listed(vec![fake(5, true)], &mut read), [5]);
    }

    #[test]
    fn elements_without_runtime_ids_are_never_taken_for_repeats() {
        let mut read = TreeRead::starting_at(Some(HOME), Vec::new());
        let blank = Fake { runtime_id: Vec::new(), inside: true };
        let kept = new_children(vec![blank.clone(), blank], &mut read, |child| child.runtime_id.clone(), |_, _| true);
        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn the_belonging_check_can_adopt_windows_for_later_children() {
        let mut read = TreeRead::starting_at(Some(HOME), Vec::new());
        let adopt_when_inside = |child: &Fake, read: &mut TreeRead| {
            if child.inside {
                read.adopt(WindowRef(i64::from(child.runtime_id[1])));
            }
            child.inside
        };
        new_children(
            vec![fake(7, true), fake(8, false)],
            &mut read,
            |child| child.runtime_id.clone(),
            adopt_when_inside,
        );
        assert_eq!(read.windows, [HOME, WindowRef(7)]);
    }

    #[test]
    fn a_level_read_lists_the_same_children_as_a_whole_read() {
        let window = taskbar().hwnd;
        let (whole, level) = handle()
            .run(move |uia| {
                let root = unsafe { uia.automation.ElementFromHandle(hwnd_of(window)) }.uia("opening the taskbar")?;
                let requests = uia.requests.props_in(View::Full);
                let whole =
                    CachedNode::new(build(&root, &requests.whole).uia("reading the whole taskbar")?, Reach::Whole);
                let level = CachedNode::new(
                    build(&root, &requests.level).uia("reading the taskbar's top level")?,
                    Reach::Level,
                );
                let count = |node: &CachedNode| {
                    let mut read = TreeRead::starting_at(Some(window), cached_runtime_id(&node.element));
                    count_nodes(uia, node, requests, &mut read)
                };
                Ok((count(&whole), count(&level)))
            })
            .unwrap();
        assert_eq!(whole, level);
    }

    fn count_nodes(uia: &Uia, node: &CachedNode, requests: &TreeRequests, read: &mut TreeRead) -> usize {
        let children = uia.tree_children(node, requests, read);
        1 + children.iter().map(|child| count_nodes(uia, child, requests, read)).sum::<usize>()
    }
}
