use std::path::Path;

use deckhand_core::{Element, ElementRef, Role, Snapshot};

pub fn folder_ready(snapshot: &Snapshot, name_box: ElementRef, folder: &str, names: &[String]) -> bool {
    address_shows(snapshot, name_box, folder) || lists_a_file(&snapshot.elements, names)
}

fn address_shows(snapshot: &Snapshot, name_box: ElementRef, folder: &str) -> bool {
    let wanted = fold_path(folder);
    !wanted.is_empty()
        && snapshot
            .elements
            .iter()
            .filter(|element| element.id != name_box && element.is_visible())
            .any(|element| fold_path(&element.name).ends_with(&wanted))
}

fn lists_a_file(elements: &[Element], names: &[String]) -> bool {
    elements
        .iter()
        .filter(|element| element.is_visible() && matches!(element.role, Role::ListItem | Role::DataItem))
        .any(|element| names.iter().any(|name| shows_file(&element.name, name)))
}

fn shows_file(shown: &str, name: &str) -> bool {
    let shown = shown.trim().to_lowercase();
    let stem = Path::new(name).file_stem().map(|stem| stem.to_string_lossy().to_lowercase());
    shown == name.to_lowercase() || stem.is_some_and(|stem| shown == stem)
}

fn fold_path(text: &str) -> String {
    text.trim().trim_end_matches(['\\', '/']).to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixture::{element, snapshot, with_id};

    const NAME_BOX: ElementRef = ElementRef(2);

    fn names() -> Vec<String> {
        vec!["first.txt".to_string(), "second.txt".to_string()]
    }

    fn dialog(extra: Vec<Element>) -> Snapshot {
        let mut typed = with_id(element(2, 1, Role::Edit, "File name:"), "1148");
        typed.value = Some("C:\\Data".to_string());
        let mut elements = vec![element(1, 0, Role::Window, "Open"), typed];
        elements.extend(extra);
        snapshot("Open", elements)
    }

    #[test]
    fn the_address_bar_naming_the_folder_confirms_it() {
        let address = element(3, 1, Role::ToolBar, "Address: C:\\Data\\");
        assert!(folder_ready(&dialog(vec![address]), NAME_BOX, "c:\\data", &names()));
        let elsewhere = element(3, 1, Role::ToolBar, "Address: C:\\Users\\me\\Documents");
        assert!(!folder_ready(&dialog(vec![elsewhere]), NAME_BOX, "C:\\Data", &names()));
    }

    #[test]
    fn the_typed_name_box_alone_proves_nothing() {
        let mut echoed = element(2, 1, Role::Edit, "C:\\Data");
        echoed.id = NAME_BOX;
        assert!(!folder_ready(&dialog(vec![echoed]), NAME_BOX, "C:\\Data", &names()));
        assert!(!folder_ready(&dialog(Vec::new()), NAME_BOX, "C:\\Data", &names()));
    }

    #[test]
    fn a_listed_file_confirms_the_folder_even_without_extensions() {
        assert!(folder_ready(
            &dialog(vec![element(4, 2, Role::ListItem, "second.txt")]),
            NAME_BOX,
            "C:\\Data",
            &names()
        ));
        assert!(folder_ready(&dialog(vec![element(4, 2, Role::ListItem, "First")]), NAME_BOX, "C:\\Data", &names()));
        assert!(!folder_ready(&dialog(vec![element(4, 2, Role::Button, "first.txt")]), NAME_BOX, "C:\\Data", &names()));
    }
}
