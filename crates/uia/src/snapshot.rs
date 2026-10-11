use std::{
    mem, thread,
    time::{Duration, Instant},
};

use deckhand_core::{Element, ElementRef, Error, Result, Snapshot, WindowInfo, WindowRef};
use windows::Win32::UI::Accessibility::IUIAutomationElement;

use crate::{
    automation::Uia, failure::UiaResult, options::SnapshotOptions, properties::NodeProps, registry::Sighting,
    tree::cached_children, win,
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
    depth: usize,
    path: Vec<u32>,
}

struct Limits {
    max_elements: usize,
    include_offscreen: bool,
    deadline: Instant,
}

impl Limits {
    fn new(options: &SnapshotOptions, started: Instant) -> Self {
        Self {
            max_elements: options.max_elements.max(1),
            include_offscreen: options.include_offscreen,
            deadline: started + options.budget,
        }
    }

    fn reached(&self, walk: &Walk) -> bool {
        let count = walk.elements.len();
        count >= self.max_elements || (count > 0 && Instant::now() >= self.deadline)
    }
}

impl Uia {
    pub fn snapshot(&mut self, window: &WindowInfo, options: &SnapshotOptions) -> Result<Snapshot> {
        let started = Instant::now();
        let mut walk = self.walk_window(window.hwnd, options, started)?;
        if is_sparse_chromium(&window.class_name, walk.elements.len()) {
            thread::sleep(CHROMIUM_WAKE_DELAY);
            walk = self.walk_window(window.hwnd, options, started)?;
        }
        self.registry.evict(Instant::now());
        Ok(Snapshot {
            window: window.clone(),
            elements: walk.elements,
            focused: walk.focused,
            truncated: walk.truncated,
        })
    }

    pub(crate) fn walk_window(
        &mut self,
        window: WindowRef,
        options: &SnapshotOptions,
        started: Instant,
    ) -> Result<Walk> {
        let root = self.cached_tree(window)?;
        Ok(self.walk_tree(window, root, &Limits::new(options, started)))
    }

    fn cached_tree(&self, window: WindowRef) -> Result<IUIAutomationElement> {
        if !win::window_exists(window) {
            return Err(Error::not_found(format!("window {window} no longer exists")));
        }
        let root = unsafe { self.automation.ElementFromHandle(win::hwnd(window)) }
            .uia(&format!("opening window {window} in UI Automation"))?;
        unsafe { root.BuildUpdatedCache(&self.requests.subtree) }
            .uia(&format!("reading the elements of window {window}"))
    }

    fn walk_tree(&mut self, window: WindowRef, root: IUIAutomationElement, limits: &Limits) -> Walk {
        let generation = self.registry.begin_snapshot(window);
        let now = Instant::now();
        let mut walk = Walk::default();
        let mut pending = vec![Pending { element: root, depth: 0, path: Vec::new() }];
        while let Some(node) = pending.pop() {
            if limits.reached(&walk) {
                walk.truncated = true;
                break;
            }
            let mut props = NodeProps::read(&node.element);
            if node.depth > 0 && props.state.offscreen && !limits.include_offscreen {
                continue;
            }
            push_children(&mut pending, &node);
            let sighting = Sighting {
                live: node.element,
                runtime_id: mem::take(&mut props.runtime_id),
                fingerprint: props.fingerprint(window, node.path),
            };
            let id = self.registry.record(sighting, generation, now);
            if props.state.focused {
                walk.focused = Some(id);
            }
            walk.elements.push(props.into_element(id, depth_of(node.depth)));
        }
        walk
    }
}

fn push_children(pending: &mut Vec<Pending>, parent: &Pending) {
    let children = cached_children(&parent.element);
    for (index, child) in children.into_iter().enumerate().rev() {
        let mut path = Vec::with_capacity(parent.path.len() + 1);
        path.extend_from_slice(&parent.path);
        path.push(u32::try_from(index).unwrap_or(u32::MAX));
        pending.push(Pending { element: child, depth: parent.depth + 1, path });
    }
}

fn depth_of(depth: usize) -> u16 {
    u16::try_from(depth).unwrap_or(u16::MAX)
}

fn is_sparse_chromium(class_name: &str, count: usize) -> bool {
    class_name.starts_with(CHROMIUM_CLASS_PREFIX) && count < SPARSE_TREE
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use deckhand_core::{ErrorCode, Role};

    use super::*;
    use crate::test_support::{foreground, handle, taskbar, window_info};

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
        let root = &snapshot.elements[0];
        assert_eq!(root.depth, 0);
        assert!(snapshot.elements[1..].iter().all(|element| element.depth > 0));
        assert!(snapshot.elements.windows(2).all(|pair| pair[1].depth <= pair[0].depth + 1));
        let unique: HashSet<ElementRef> = snapshot.elements.iter().map(|element| element.id).collect();
        assert_eq!(unique.len(), snapshot.elements.len());
        assert!(!root.rect.is_empty());
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
        assert_eq!(first.elements[0].id, second.elements[0].id);
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
        assert_eq!(snapshot.elements[0].depth, 0);
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
                    assert_eq!(snapshot.elements[0].depth, 0);
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
