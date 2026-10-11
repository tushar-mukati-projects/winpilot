use std::cmp::{Ordering, Reverse};

use deckhand_core::{ElementRef, Role, WindowRef};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Fingerprint {
    pub(crate) window: WindowRef,
    pub(crate) role: Role,
    pub(crate) name: String,
    pub(crate) automation_id: String,
    pub(crate) class_name: String,
    pub(crate) path: Vec<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Closeness {
    same_class: bool,
    same_path: bool,
    shared_prefix: usize,
    length_gap: Reverse<usize>,
}

impl Fingerprint {
    pub(crate) fn depth(&self) -> u16 {
        u16::try_from(self.path.len()).unwrap_or(u16::MAX)
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
        other.name.is_empty() && other.class_name == self.class_name && other.path == self.path
    }

    pub(crate) fn closeness(&self, other: &Self) -> Closeness {
        Closeness {
            same_class: other.class_name == self.class_name,
            same_path: other.path == self.path,
            shared_prefix: self.path.iter().zip(&other.path).take_while(|(a, b)| a == b).count(),
            length_gap: Reverse(self.path.len().abs_diff(other.path.len())),
        }
    }
}

pub(crate) fn best_match<'a>(
    target: &Fingerprint,
    candidates: impl Iterator<Item = (ElementRef, &'a Fingerprint)>,
) -> Option<ElementRef> {
    candidates
        .filter(|(_, candidate)| target.could_be(candidate))
        .max_by(|(left_id, left), (right_id, right)| rank(target, (*left_id, left), (*right_id, right)))
        .map(|(id, _)| id)
}

fn rank(target: &Fingerprint, left: (ElementRef, &Fingerprint), right: (ElementRef, &Fingerprint)) -> Ordering {
    target.closeness(left.1).cmp(&target.closeness(right.1)).then(right.0.cmp(&left.0))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn print(role: Role, name: &str, automation_id: &str, class_name: &str, path: &[u32]) -> Fingerprint {
        Fingerprint {
            window: WindowRef(7),
            role,
            name: name.to_string(),
            automation_id: automation_id.to_string(),
            class_name: class_name.to_string(),
            path: path.to_vec(),
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
    fn anonymous_elements_need_class_and_path() {
        let target = print(Role::Pane, "", "", "Pane", &[0, 1]);
        assert!(target.could_be(&print(Role::Pane, "", "", "Pane", &[0, 1])));
        assert!(!target.could_be(&print(Role::Pane, "", "", "Pane", &[0, 2])));
        assert!(!target.could_be(&print(Role::Pane, "", "", "Other", &[0, 1])));
    }

    #[test]
    fn other_windows_never_match() {
        let target = print(Role::Button, "Save", "", "Button", &[0]);
        let mut elsewhere = target.clone();
        elsewhere.window = WindowRef(8);
        assert!(!target.could_be(&elsewhere));
    }

    #[test]
    fn ties_break_by_class_then_path() {
        let target = print(Role::Button, "Save", "", "Button", &[0, 4, 2]);
        let wrong_class = print(Role::Button, "Save", "", "Other", &[0, 4, 2]);
        let far = print(Role::Button, "Save", "", "Button", &[1, 0]);
        let near = print(Role::Button, "Save", "", "Button", &[0, 4, 3]);
        let candidates = [(ElementRef(1), &wrong_class), (ElementRef(2), &far), (ElementRef(3), &near)];
        assert_eq!(best_match(&target, candidates.into_iter()), Some(ElementRef(3)));
    }

    #[test]
    fn equal_candidates_pick_the_oldest_ref() {
        let target = print(Role::Button, "Save", "", "Button", &[0]);
        let a = print(Role::Button, "Save", "", "Button", &[1]);
        let b = print(Role::Button, "Save", "", "Button", &[1]);
        let candidates = [(ElementRef(9), &a), (ElementRef(4), &b)];
        assert_eq!(best_match(&target, candidates.into_iter()), Some(ElementRef(4)));
    }

    #[test]
    fn no_candidates_means_no_match() {
        let target = print(Role::Button, "Save", "", "Button", &[0]);
        let other = print(Role::Button, "Open", "", "Button", &[0]);
        assert_eq!(best_match(&target, [(ElementRef(1), &other)].into_iter()), None);
    }
}
