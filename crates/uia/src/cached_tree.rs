use std::collections::HashSet;

use deckhand_core::WindowRef;
use windows::Win32::{
    Foundation::{E_FAIL, HWND},
    UI::{
        Accessibility::{IUIAutomationCacheRequest, IUIAutomationElement},
        WindowsAndMessaging::{GA_PARENT, GetAncestor, GetDesktopWindow},
    },
};

use crate::{automation::Uia, properties::cached_runtime_id, tree::cached_children, win, win32_combo::dropdown_list};

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
}

impl TreeRead {
    fn starting_at(home: Option<WindowRef>, root_runtime_id: Vec<i32>) -> Self {
        let mut read = Self { windows: home.into_iter().collect(), listed: HashSet::new() };
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
}

impl Uia {
    pub(crate) fn read_tree(
        &self,
        root: &IUIAutomationElement,
        requests: TreeRequests<'_>,
    ) -> windows_core::Result<(CachedNode, TreeRead)> {
        let node = first_read(root, requests)?;
        let home = self.owning_window(&node.element, own_window(&node.element));
        let read = TreeRead::starting_at(home, cached_runtime_id(&node.element));
        Ok((node, read))
    }

    pub(crate) fn tree_children(
        &self,
        node: &CachedNode,
        requests: TreeRequests<'_>,
        read: &mut TreeRead,
    ) -> Vec<CachedNode> {
        let reach = match node.reach {
            Reach::Whole => Reach::Whole,
            Reach::Level => Reach::Unread,
            Reach::Unread => return self.tree_children(&read_further(&node.element, requests), requests, read),
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

fn first_read(root: &IUIAutomationElement, requests: TreeRequests<'_>) -> windows_core::Result<CachedNode> {
    match build(root, requests.whole) {
        Ok(whole) => Ok(CachedNode::new(whole, Reach::Whole)),
        Err(error) if is_broken_tree(&error) => {
            build(root, requests.level).map(|level| CachedNode::new(level, Reach::Level)).map_err(|_| error)
        }
        Err(error) => Err(error),
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
    error.code() == E_FAIL
}

fn own_window(element: &IUIAutomationElement) -> WindowRef {
    cached_window_handle(element).map_or(WindowRef(0), win::window_ref)
}

fn cached_window_handle(element: &IUIAutomationElement) -> Option<HWND> {
    unsafe { element.CachedNativeWindowHandle() }.ok().filter(|window| !window.is_invalid())
}

fn native_window_of(element: &IUIAutomationElement) -> NativeWindow {
    let Some(window) = cached_window_handle(element) else { return NativeWindow::Missing };
    if unsafe { GetAncestor(window, GA_PARENT) } == unsafe { GetDesktopWindow() } {
        NativeWindow::TopLevel(win::window_ref(window))
    } else {
        NativeWindow::Child
    }
}

fn is_dropdown_of(parent: &IUIAutomationElement, window: WindowRef) -> bool {
    if win::class_name(window).as_deref() != Some(DROPDOWN_LIST_CLASS) {
        return false;
    }
    let Some(combo) = cached_window_handle(parent) else { return false };
    dropdown_list(combo).map(win::window_ref) == Some(window)
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

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

    #[test]
    fn only_unexpected_failures_count_as_a_broken_tree() {
        let failure = |code: u32| windows_core::Error::from_hresult(windows_core::HRESULT(code as i32));
        assert!(is_broken_tree(&failure(0x8000_4005)));
        assert!(!is_broken_tree(&failure(0x8004_0201)));
        assert!(!is_broken_tree(&failure(0x8013_1505)));
        assert!(!is_broken_tree(&failure(0x8007_0005)));
        assert!(!is_broken_tree(&failure(0x8007_000E)));
        assert!(!is_broken_tree(&failure(0x8007_0057)));
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
                let root = unsafe { uia.automation.ElementFromHandle(win::hwnd(window)) }.uia("opening the taskbar")?;
                let requests = uia.requests.props_in(View::Full);
                let whole =
                    CachedNode::new(build(&root, requests.whole).uia("reading the whole taskbar")?, Reach::Whole);
                let level =
                    CachedNode::new(build(&root, requests.level).uia("reading the taskbar's top level")?, Reach::Level);
                let count = |node: &CachedNode| {
                    let mut read = TreeRead::starting_at(Some(window), cached_runtime_id(&node.element));
                    count_nodes(uia, node, requests, &mut read)
                };
                Ok((count(&whole), count(&level)))
            })
            .unwrap();
        assert_eq!(whole, level);
    }

    fn count_nodes(uia: &Uia, node: &CachedNode, requests: TreeRequests<'_>, read: &mut TreeRead) -> usize {
        let children = uia.tree_children(node, requests, read);
        1 + children.iter().map(|child| count_nodes(uia, child, requests, read)).sum::<usize>()
    }
}
