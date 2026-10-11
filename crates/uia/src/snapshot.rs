use std::{
    mem, thread,
    time::{Duration, Instant},
};

use deckhand_core::{Element, ElementRef, Error, Result, Snapshot, WindowInfo, WindowRef};
use windows::Win32::UI::Accessibility::IUIAutomationElement;

use crate::{
    automation::Uia,
    failure::UiaResult,
    options::SnapshotOptions,
    properties::NodeProps,
    registry::Sighting,
    tree::cached_children,
    tree_path::{TreePath, View},
    win,
    window_identity::WindowIdentity,
};

const CHROMIUM_CLASS_PREFIX: &str = "Chrome_WidgetWin";
const SPARSE_TREE: usize = 8;
const CHROMIUM_WAKE_DELAY: Duration = Duration::from_millis(150);

#[derive(Default)]
pub(crate) struct Walk {
    pub(crate) elements: Vec<Element>,
    pub(crate) focused: Option<ElementRef>,
    pub(crate) truncated: bool,
}

struct Pending {
    element: IUIAutomationElement,
    path: TreePath,
}

struct Limits {
    max_elements: usize,
    deadline: Instant,
}

impl Limits {
    fn starting_now(options: &SnapshotOptions) -> Self {
        Self { max_elements: options.max_elements.max(1), deadline: Instant::now() + options.budget }
    }

    fn reached(&self, walk: &Walk) -> bool {
        let count = walk.elements.len();
        count >= self.max_elements || (count > 0 && Instant::now() >= self.deadline)
    }
}

impl Uia {
    pub fn snapshot(&mut self, window: &WindowInfo, options: &SnapshotOptions) -> Result<Snapshot> {
        let identity = WindowIdentity::read(window.hwnd).ok_or_else(|| window_gone(window.hwnd))?;
        self.registry.begin_snapshot(window.hwnd);
        let walk = self.walk_with_chromium_retry(&identity, options)?;
        self.evict_stale_refs();
        let focused = walk.focused.filter(|_| win::is_foreground(window.hwnd));
        Ok(Snapshot { window: window.clone(), elements: walk.elements, focused, truncated: walk.truncated })
    }

    pub(crate) fn walk_window(&mut self, window: &WindowIdentity, options: &SnapshotOptions) -> Result<Walk> {
        let view = View::of(options);
        let root = self.cached_tree(window.hwnd, view)?;
        let limits = Limits::starting_now(options);
        Ok(self.walk_tree(window, root, view, &limits))
    }

    fn walk_with_chromium_retry(&mut self, window: &WindowIdentity, options: &SnapshotOptions) -> Result<Walk> {
        let first = self.walk_window(window, options)?;
        if !self.needs_wake_up(window, first.elements.len()) {
            return Ok(first);
        }
        thread::sleep(CHROMIUM_WAKE_DELAY);
        Ok(match self.walk_window(window, options) {
            Ok(second) if second.elements.len() >= first.elements.len() => second,
            _ => first,
        })
    }

    fn needs_wake_up(&mut self, window: &WindowIdentity, count: usize) -> bool {
        if win::is_minimized(window.hwnd) {
            return false;
        }
        self.warmed.first_visit(window.hwnd) && is_sparse_chromium(&window.class, count)
    }

    fn cached_tree(&self, window: WindowRef, view: View) -> Result<IUIAutomationElement> {
        if !win::window_exists(window) {
            return Err(window_gone(window));
        }
        let root = unsafe { self.automation.ElementFromHandle(win::hwnd(window)) }
            .uia(&format!("opening window {window} in UI Automation"))?;
        unsafe { root.BuildUpdatedCache(self.requests.subtree_props_in(view)) }
            .uia(&format!("reading the elements of window {window}"))
    }

    fn walk_tree(&mut self, window: &WindowIdentity, root: IUIAutomationElement, view: View, limits: &Limits) -> Walk {
        let generation = self.registry.generation(window.hwnd);
        let now = Instant::now();
        let mut walk = Walk::default();
        let mut pending = vec![Pending { element: root, path: TreePath::root(view) }];
        while let Some(node) = pending.pop() {
            if limits.reached(&walk) {
                walk.truncated = true;
                break;
            }
            push_children(&mut pending, &node);
            let mut props = NodeProps::read(&node.element);
            let depth = node.path.depth();
            let runtime_id = mem::take(&mut props.runtime_id);
            let fingerprint = props.fingerprint(window.clone(), Some(node.path));
            let id = self.registry.record(Sighting { live: node.element, runtime_id, fingerprint }, generation, now);
            if props.state.focused {
                walk.focused = Some(id);
            }
            walk.elements.push(props.into_element(id, depth));
        }
        walk
    }
}

fn push_children(pending: &mut Vec<Pending>, parent: &Pending) {
    let children = cached_children(&parent.element);
    for (index, child) in children.into_iter().enumerate().rev() {
        pending.push(Pending { element: child, path: parent.path.child(index) });
    }
}

fn window_gone(window: WindowRef) -> Error {
    Error::not_found(format!("window {window} no longer exists"))
}

fn is_sparse_chromium(class_name: &str, count: usize) -> bool {
    class_name.starts_with(CHROMIUM_CLASS_PREFIX) && count < SPARSE_TREE
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use deckhand_core::{ErrorCode, Role};

    use super::*;
    use crate::test_support::{first_element, foreground, handle, taskbar, window_info};

    fn assert_pre_order(elements: &[Element]) {
        let Some((root, rest)) = elements.split_first() else { panic!("empty snapshot") };
        assert_eq!(root.depth, 0);
        assert!(rest.iter().all(|element| element.depth > 0));
        assert!(elements.windows(2).all(|pair| pair[1].depth <= pair[0].depth + 1));
    }

    #[test]
    fn only_sparse_chromium_windows_retry() {
        assert!(is_sparse_chromium("Chrome_WidgetWin_1", 3));
        assert!(!is_sparse_chromium("Chrome_WidgetWin_1", 8));
        assert!(!is_sparse_chromium("Notepad", 1));
    }

    #[test]
    fn taskbar_snapshot_starts_at_the_root() {
        let window = taskbar();
        let snapshot = handle().run(move |uia| uia.snapshot(&window, &SnapshotOptions::default())).unwrap();
        assert_pre_order(&snapshot.elements);
        let unique: HashSet<ElementRef> = snapshot.elements.iter().map(|element| element.id).collect();
        assert_eq!(unique.len(), snapshot.elements.len());
        assert!(!first_element(&snapshot).unwrap().rect.is_empty());
    }

    #[test]
    fn onscreen_snapshots_leave_offscreen_elements_out() {
        let Some(window) = foreground() else { return };
        let full = SnapshotOptions { include_offscreen: true, ..SnapshotOptions::default() };
        let snapshots = handle()
            .run(move |uia| Ok((uia.snapshot(&window, &SnapshotOptions::default()), uia.snapshot(&window, &full))));
        let (Ok(onscreen), Ok(everything)) = snapshots.unwrap() else { return };
        assert_pre_order(&onscreen.elements);
        assert_pre_order(&everything.elements);
        assert!(onscreen.elements.iter().skip(1).all(|element| !element.state.offscreen));
        eprintln!("on-screen {} of {} elements", onscreen.elements.len(), everything.elements.len());
    }

    #[test]
    fn background_windows_report_no_focus() {
        let window = taskbar();
        let hwnd = window.hwnd;
        let snapshot = handle().run(move |uia| uia.snapshot(&window, &SnapshotOptions::default())).unwrap();
        if !win::is_foreground(hwnd) {
            assert_eq!(snapshot.focused, None);
        }
    }

    #[test]
    fn refs_stay_stable_across_snapshots() {
        let window = taskbar();
        let (first, second) = handle()
            .run(move |uia| {
                let options = SnapshotOptions::default();
                Ok((uia.snapshot(&window, &options)?, uia.snapshot(&window, &options)?))
            })
            .unwrap();
        assert_eq!(first_element(&first).unwrap().id, first_element(&second).unwrap().id);
        let earlier: HashSet<ElementRef> = first.elements.iter().map(|element| element.id).collect();
        let kept = second.elements.iter().filter(|element| earlier.contains(&element.id)).count();
        assert!(kept * 2 >= second.elements.len(), "only {kept} of {} refs survived", second.elements.len());
    }

    #[test]
    fn max_elements_truncates() {
        let window = taskbar();
        let options = SnapshotOptions { max_elements: 2, include_offscreen: true, ..SnapshotOptions::default() };
        let snapshot = handle().run(move |uia| uia.snapshot(&window, &options)).unwrap();
        assert!(snapshot.elements.len() <= 2);
        if snapshot.elements.len() == 2 {
            assert!(snapshot.truncated);
        }
    }

    #[test]
    fn zero_budget_still_keeps_the_root() {
        let window = taskbar();
        let options = SnapshotOptions { budget: Duration::ZERO, ..SnapshotOptions::default() };
        let snapshot = handle().run(move |uia| uia.snapshot(&window, &options)).unwrap();
        assert_eq!(snapshot.elements.len(), 1);
        assert_eq!(first_element(&snapshot).unwrap().depth, 0);
    }

    #[test]
    fn missing_window_is_not_found() {
        let window = window_info(WindowRef(0), "Nothing");
        let error = handle().run(move |uia| uia.snapshot(&window, &SnapshotOptions::default())).unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
    }

    #[test]
    fn foreground_snapshot_reports_timings() {
        let Some(window) = foreground() else { return };
        let uia = handle();
        let measured = uia.run(move |uia| {
            let mut timings = Vec::new();
            for _ in 0..3 {
                let started = Instant::now();
                let snapshot = uia.snapshot(&window, &SnapshotOptions::default());
                timings.push((started.elapsed(), snapshot));
            }
            Ok(timings)
        });
        for (elapsed, snapshot) in measured.unwrap() {
            match snapshot {
                Ok(snapshot) => {
                    assert_pre_order(&snapshot.elements);
                    eprintln!("foreground snapshot: {} elements in {:?}", snapshot.elements.len(), elapsed);
                }
                Err(error) => assert!(
                    matches!(error.code, ErrorCode::ElevatedTarget | ErrorCode::NotFound),
                    "unexpected {}",
                    error.code
                ),
            }
        }
    }

    #[test]
    fn taskbar_snapshot_reports_timings() {
        let window = taskbar();
        let timings = handle()
            .run(move |uia| {
                let mut timings = Vec::new();
                for _ in 0..5 {
                    let started = Instant::now();
                    let snapshot = uia.snapshot(&window, &SnapshotOptions::default())?;
                    timings.push((snapshot.elements.len(), started.elapsed()));
                }
                Ok(timings)
            })
            .unwrap();
        for (count, elapsed) in timings {
            eprintln!("taskbar snapshot: {count} elements in {elapsed:?}");
        }
    }

    #[test]
    fn snapshot_roles_are_mapped() {
        let window = taskbar();
        let snapshot = handle().run(move |uia| uia.snapshot(&window, &SnapshotOptions::default())).unwrap();
        assert!(snapshot.elements.iter().any(|element| element.role != Role::Unknown));
    }
}
