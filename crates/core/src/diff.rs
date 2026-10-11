use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::{
    element::{Element, ElementState, Snapshot},
    refs::ElementRef,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Change {
    Added { element: Element },
    Removed { element: Element },
    Changed { before: Element, after: Element },
}

pub fn diff(before: &Snapshot, after: &Snapshot) -> Vec<Change> {
    let previous: HashMap<ElementRef, &Element> = before.elements.iter().map(|element| (element.id, element)).collect();
    let mut changes: Vec<Change> =
        after.elements.iter().filter_map(|element| compare(previous.get(&element.id).copied(), element)).collect();
    let current: HashSet<ElementRef> = after.elements.iter().map(|element| element.id).collect();
    changes.extend(
        before
            .elements
            .iter()
            .filter(|element| !current.contains(&element.id))
            .map(|element| Change::Removed { element: element.clone() }),
    );
    changes
}

fn compare(before: Option<&Element>, after: &Element) -> Option<Change> {
    let Some(before) = before else {
        return Some(Change::Added { element: after.clone() });
    };
    differs(before, after).then(|| Change::Changed { before: before.clone(), after: after.clone() })
}

fn differs(before: &Element, after: &Element) -> bool {
    before.name != after.name
        || before.value != after.value
        || before.role != after.role
        || before.automation_id != after.automation_id
        || ignoring_focus(before.state) != ignoring_focus(after.state)
}

const fn ignoring_focus(state: ElementState) -> ElementState {
    ElementState { focused: false, ..state }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        element::fixtures::{element, snapshot},
        geometry::Rect,
        role::Role,
    };

    fn ids(changes: &[Change]) -> Vec<(char, u32)> {
        changes
            .iter()
            .map(|change| match change {
                Change::Added { element } => ('+', element.id.0),
                Change::Removed { element } => ('-', element.id.0),
                Change::Changed { after, .. } => ('~', after.id.0),
            })
            .collect()
    }

    fn base() -> Vec<Element> {
        vec![
            element(1, 0, Role::Window, "Notepad"),
            element(2, 1, Role::Edit, "Text editor"),
            element(3, 1, Role::Button, "Close"),
        ]
    }

    #[test]
    fn identical_snapshots_have_no_changes() {
        assert!(diff(&snapshot(base()), &snapshot(base())).is_empty());
    }

    #[test]
    fn empty_snapshots_have_no_changes() {
        assert!(diff(&snapshot(Vec::new()), &snapshot(Vec::new())).is_empty());
    }

    #[test]
    fn reports_added_elements() {
        let mut after = base();
        after.push(element(4, 1, Role::Button, "Save"));
        let changes = diff(&snapshot(base()), &snapshot(after));
        assert_eq!(changes, [Change::Added { element: element(4, 1, Role::Button, "Save") }]);
    }

    #[test]
    fn reports_removed_elements() {
        let mut after = base();
        after.remove(2);
        let changes = diff(&snapshot(base()), &snapshot(after));
        assert_eq!(changes, [Change::Removed { element: element(3, 1, Role::Button, "Close") }]);
    }

    #[test]
    fn reports_value_changes_with_both_sides() {
        let mut after = base();
        after[1].value = Some("hello".to_string());
        let changes = diff(&snapshot(base()), &snapshot(after.clone()));
        assert_eq!(changes, [Change::Changed { before: base()[1].clone(), after: after[1].clone() }]);
    }

    #[test]
    fn each_compared_field_counts_as_a_change() {
        let edits: [fn(&mut Element); 10] = [
            |element| element.name = "Renamed".to_string(),
            |element| element.value = Some(String::new()),
            |element| element.role = Role::Combobox,
            |element| element.automation_id = "15".to_string(),
            |element| element.state.enabled = false,
            |element| element.state.offscreen = true,
            |element| element.state.read_only = true,
            |element| element.state.toggled = Some(true),
            |element| element.state.expanded = Some(false),
            |element| element.state.selected = Some(true),
        ];
        for edit in edits {
            let mut after = base();
            edit(&mut after[1]);
            assert_eq!(ids(&diff(&snapshot(base()), &snapshot(after))), [('~', 2)]);
        }
    }

    #[test]
    fn password_and_focusable_state_count_as_changes() {
        let mut after = base();
        after[1].state.password = true;
        after[2].state.focusable = true;
        assert_eq!(ids(&diff(&snapshot(base()), &snapshot(after))), [('~', 2), ('~', 3)]);
    }

    #[test]
    fn focus_only_changes_are_ignored() {
        let mut before = base();
        before[1].state.focused = true;
        let mut after = base();
        after[2].state.focused = true;
        assert!(diff(&snapshot(before), &snapshot(after)).is_empty());
    }

    #[test]
    fn rect_only_changes_are_ignored() {
        let mut after = base();
        for element in &mut after {
            element.rect = Rect::new(element.rect.x, element.rect.y - 300, 50, 20);
        }
        assert!(diff(&snapshot(base()), &snapshot(after)).is_empty());
    }

    #[test]
    fn depth_patterns_and_class_changes_are_ignored() {
        let mut after = base();
        after[1].depth = 4;
        after[1].patterns.value = true;
        after[1].class_name = "RichEdit".to_string();
        assert!(diff(&snapshot(base()), &snapshot(after)).is_empty());
    }

    #[test]
    fn focus_change_alongside_a_real_change_is_reported() {
        let mut after = base();
        after[1].state.focused = true;
        after[1].value = Some("x".to_string());
        assert_eq!(ids(&diff(&snapshot(base()), &snapshot(after))), [('~', 2)]);
    }

    #[test]
    fn order_is_after_document_order_then_removals_in_before_order() {
        let before = vec![
            element(1, 0, Role::Window, "Notepad"),
            element(2, 1, Role::Button, "Gone first"),
            element(3, 1, Role::Edit, "Text editor"),
            element(4, 1, Role::Button, "Gone second"),
        ];
        let mut edited = element(3, 1, Role::Edit, "Text editor");
        edited.value = Some("typed".to_string());
        let after = vec![
            element(9, 1, Role::Button, "New top"),
            element(1, 0, Role::Window, "Notepad"),
            edited,
            element(7, 1, Role::Button, "New bottom"),
        ];
        let changes = diff(&snapshot(before), &snapshot(after));
        assert_eq!(ids(&changes), [('+', 9), ('~', 3), ('+', 7), ('-', 2), ('-', 4)]);
    }

    #[test]
    fn everything_replaced() {
        let after = vec![element(10, 0, Role::Window, "Other"), element(11, 1, Role::Button, "OK")];
        let changes = diff(&snapshot(base()), &snapshot(after));
        assert_eq!(ids(&changes), [('+', 10), ('+', 11), ('-', 1), ('-', 2), ('-', 3)]);
    }
}
