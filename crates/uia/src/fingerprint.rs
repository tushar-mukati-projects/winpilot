use deckhand_core::{ElementRef, Role};

use crate::{
    tree_path::{PathCloseness, TreePath, View},
    window_identity::WindowIdentity,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fingerprint {
    pub(crate) window: WindowIdentity,
    pub(crate) role: Role,
    pub(crate) name: String,
    pub(crate) automation_id: String,
    pub(crate) class_name: String,
    pub(crate) path: Option<TreePath>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Closeness {
    same_class: bool,
    path: PathCloseness,
}

impl Fingerprint {
    pub(crate) fn depth(&self) -> u16 {
        self.path.as_ref().map_or(0, TreePath::depth)
    }

    pub(crate) fn view(&self) -> View {
        self.path.as_ref().map_or(View::Full, |path| path.view)
    }

    pub(crate) fn keeps_identity(&self, role: Role, automation_id: &str) -> bool {
        role == self.role && (self.automation_id.is_empty() || automation_id == self.automation_id)
    }

    pub(crate) fn could_be(&self, other: &Self) -> bool {
        if other.window != self.window || other.role != self.role {
            return false;
        }
        if !self.automation_id.is_empty() {
            return other.automation_id == self.automation_id;
        }
        if !self.name.is_empty() {
            return other.name == self.name;
        }
        other.name.is_empty() && other.class_name == self.class_name && self.path.is_some() && other.path == self.path
    }

    fn closeness(&self, other: &Self) -> Closeness {
        Closeness {
            same_class: other.class_name == self.class_name,
            path: PathCloseness::between(self.path.as_ref(), other.path.as_ref()),
        }
    }
}

pub(crate) fn top_matches<'a>(
    target: &Fingerprint,
    candidates: impl Iterator<Item = (ElementRef, &'a Fingerprint)>,
) -> Vec<ElementRef> {
    let plausible: Vec<(Closeness, ElementRef)> = candidates
        .filter(|(_, candidate)| target.could_be(candidate))
        .map(|(id, candidate)| (target.closeness(candidate), id))
        .collect();
    let Some(best) = plausible.iter().map(|(closeness, _)| *closeness).max() else { return Vec::new() };
    plausible.into_iter().filter(|(closeness, _)| *closeness == best).map(|(_, id)| id).collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Arc;

    use deckhand_core::WindowRef;

    use super::*;

    pub(crate) fn test_window(hwnd: i64) -> WindowIdentity {
        WindowIdentity { hwnd: WindowRef(hwnd), pid: 4242, class: Arc::from("TestWindow") }
    }

    pub(crate) fn print(role: Role, name: &str, automation_id: &str, class_name: &str, path: &[u32]) -> Fingerprint {
        Fingerprint {
            window: test_window(7),
            role,
            name: name.to_string(),
            automation_id: automation_id.to_string(),
            class_name: class_name.to_string(),
            path: Some(TreePath { view: View::Full, indices: path.to_vec() }),
        }
    }

    #[test]
    fn automation_id_wins_over_name() {
        let target = print(Role::Button, "Save", "save-btn", "Button", &[0, 1]);
        assert!(target.could_be(&print(Role::Button, "Save changes", "save-btn", "Button", &[0, 2])));
        assert!(!target.could_be(&print(Role::Button, "Save", "other", "Button", &[0, 1])));
    }

    #[test]
    fn name_is_used_without_automation_id() {
        let target = print(Role::Button, "Save", "", "Button", &[0, 1]);
        assert!(target.could_be(&print(Role::Button, "Save", "x", "Other", &[3])));
        assert!(!target.could_be(&print(Role::Button, "Open", "", "Button", &[0, 1])));
        assert!(!target.could_be(&print(Role::Link, "Save", "", "Button", &[0, 1])));
    }

    #[test]
    fn anonymous_elements_need_class_and_exact_path() {
        let target = print(Role::Pane, "", "", "Pane", &[0, 1]);
        assert!(target.could_be(&print(Role::Pane, "", "", "Pane", &[0, 1])));
        assert!(!target.could_be(&print(Role::Pane, "", "", "Pane", &[0, 2])));
        assert!(!target.could_be(&print(Role::Pane, "", "", "Other", &[0, 1])));
        let mut onscreen = print(Role::Pane, "", "", "Pane", &[0, 1]);
        onscreen.path = Some(TreePath { view: View::Onscreen, indices: vec![0, 1] });
        assert!(!target.could_be(&onscreen));
    }

    #[test]
    fn anonymous_lookups_never_match() {
        let mut target = print(Role::Pane, "", "", "Pane", &[]);
        target.path = None;
        let mut root = print(Role::Pane, "", "", "Pane", &[]);
        assert!(!target.could_be(&root));
        root.path = None;
        assert!(!target.could_be(&root));
        assert_eq!(target.depth(), 0);
        assert_eq!(target.view(), View::Full);
    }

    #[test]
    fn other_windows_never_match() {
        let target = print(Role::Button, "Save", "", "Button", &[0]);
        let mut elsewhere = target.clone();
        elsewhere.window = test_window(8);
        assert!(!target.could_be(&elsewhere));
        let mut reused = target.clone();
        reused.window.pid += 1;
        assert!(!target.could_be(&reused));
    }

    #[test]
    fn identity_survives_renames_but_not_role_or_id_changes() {
        let target = print(Role::Button, "Play", "play", "Button", &[0]);
        assert!(target.keeps_identity(Role::Button, "play"));
        assert!(!target.keeps_identity(Role::Checkbox, "play"));
        assert!(!target.keeps_identity(Role::Button, "stop"));
        let anonymous = print(Role::Button, "Play", "", "Button", &[0]);
        assert!(anonymous.keeps_identity(Role::Button, "gained-an-id"));
    }

    #[test]
    fn the_closest_candidate_wins_alone() {
        let target = print(Role::Button, "Save", "", "Button", &[0, 4, 2]);
        let wrong_class = print(Role::Button, "Save", "", "Other", &[0, 4, 2]);
        let far = print(Role::Button, "Save", "", "Button", &[1, 0]);
        let near = print(Role::Button, "Save", "", "Button", &[0, 4, 3]);
        let candidates = [(ElementRef(1), &wrong_class), (ElementRef(2), &far), (ElementRef(3), &near)];
        assert_eq!(top_matches(&target, candidates.into_iter()), [ElementRef(3)]);
    }

    #[test]
    fn equally_close_candidates_are_all_returned() {
        let target = print(Role::Button, "Save", "", "Button", &[0]);
        let a = print(Role::Button, "Save", "", "Button", &[1]);
        let b = print(Role::Button, "Save", "", "Button", &[1]);
        let candidates = [(ElementRef(9), &a), (ElementRef(4), &b)];
        assert_eq!(top_matches(&target, candidates.into_iter()), [ElementRef(9), ElementRef(4)]);
    }

    #[test]
    fn lookups_without_a_path_tie_on_every_position() {
        let mut target = print(Role::Button, "Save", "", "Button", &[]);
        target.path = None;
        let shallow = print(Role::Button, "Save", "", "Button", &[0]);
        let deep = print(Role::Button, "Save", "", "Button", &[0, 3, 1]);
        let candidates = [(ElementRef(1), &shallow), (ElementRef(2), &deep)];
        assert_eq!(top_matches(&target, candidates.into_iter()), [ElementRef(1), ElementRef(2)]);
    }

    #[test]
    fn no_candidates_means_no_match() {
        let target = print(Role::Button, "Save", "", "Button", &[0]);
        let other = print(Role::Button, "Open", "", "Button", &[0]);
        assert!(top_matches(&target, [(ElementRef(1), &other)].into_iter()).is_empty());
    }
}
