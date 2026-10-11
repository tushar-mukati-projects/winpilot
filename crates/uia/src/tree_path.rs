use std::cmp::Reverse;

use crate::options::SnapshotOptions;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum View {
    Onscreen,
    Full,
}

impl View {
    pub(crate) fn of(options: &SnapshotOptions) -> Self {
        if options.include_offscreen { Self::Full } else { Self::Onscreen }
    }

    pub(crate) fn includes_offscreen(self) -> bool {
        self == Self::Full
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TreePath {
    pub(crate) view: View,
    pub(crate) indices: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct PathCloseness {
    same: bool,
    shared_prefix: usize,
    length_gap: Reverse<usize>,
}

impl TreePath {
    pub(crate) fn root(view: View) -> Self {
        Self { view, indices: Vec::new() }
    }

    pub(crate) fn child(&self, index: usize) -> Self {
        let mut indices = Vec::with_capacity(self.indices.len() + 1);
        indices.extend_from_slice(&self.indices);
        indices.push(u32::try_from(index).unwrap_or(u32::MAX));
        Self { view: self.view, indices }
    }

    pub(crate) fn depth(&self) -> u16 {
        u16::try_from(self.indices.len()).unwrap_or(u16::MAX)
    }
}

impl PathCloseness {
    const UNKNOWN: Self = Self { same: false, shared_prefix: 0, length_gap: Reverse(usize::MAX) };

    pub(crate) fn between(target: Option<&TreePath>, candidate: Option<&TreePath>) -> Self {
        match (target, candidate) {
            (Some(target), Some(candidate)) if target.view == candidate.view => {
                Self::compare(&target.indices, &candidate.indices)
            }
            _ => Self::UNKNOWN,
        }
    }

    fn compare(target: &[u32], candidate: &[u32]) -> Self {
        Self {
            same: target == candidate,
            shared_prefix: target.iter().zip(candidate).take_while(|(a, b)| a == b).count(),
            length_gap: Reverse(target.len().abs_diff(candidate.len())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn path(view: View, indices: &[u32]) -> TreePath {
        TreePath { view, indices: indices.to_vec() }
    }

    #[test]
    fn children_extend_the_parent_path() {
        let child = TreePath::root(View::Onscreen).child(2).child(0);
        assert_eq!(child, path(View::Onscreen, &[2, 0]));
        assert_eq!(child.depth(), 2);
    }

    #[test]
    fn the_view_follows_the_offscreen_option() {
        let all = SnapshotOptions { include_offscreen: true, ..SnapshotOptions::default() };
        assert_eq!(View::of(&all), View::Full);
        assert_eq!(View::of(&SnapshotOptions::default()), View::Onscreen);
        assert!(View::Full.includes_offscreen());
        assert!(!View::Onscreen.includes_offscreen());
    }

    #[test]
    fn closer_paths_rank_higher() {
        let target = path(View::Full, &[0, 4, 2]);
        let same = PathCloseness::between(Some(&target), Some(&path(View::Full, &[0, 4, 2])));
        let near = PathCloseness::between(Some(&target), Some(&path(View::Full, &[0, 4, 3])));
        let far = PathCloseness::between(Some(&target), Some(&path(View::Full, &[1, 0])));
        assert!(same > near && near > far);
    }

    #[test]
    fn paths_from_other_views_or_lookups_say_nothing() {
        let target = path(View::Onscreen, &[0, 1]);
        let other_view = PathCloseness::between(Some(&target), Some(&path(View::Full, &[0, 1])));
        let lookup = PathCloseness::between(None, Some(&path(View::Full, &[0, 1])));
        assert_eq!(other_view, PathCloseness::UNKNOWN);
        assert_eq!(lookup, PathCloseness::UNKNOWN);
    }
}
