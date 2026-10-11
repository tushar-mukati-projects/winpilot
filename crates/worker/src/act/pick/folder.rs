use std::path::Path;

use deckhand_core::{Element, ElementRef, Role, Snapshot};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Destination {
    pub folder: String,
    pub names: Vec<String>,
    pub listed_before: Vec<String>,
}

pub fn folder_ready(snapshot: &Snapshot, name_box: ElementRef, destination: &Destination) -> bool {
    address_shows(snapshot, name_box, &destination.folder) || lists_a_new_file(&snapshot.elements, destination)
}

pub fn listed_files(elements: &[Element]) -> Vec<String> {
    let mut names: Vec<String> = elements
        .iter()
        .filter(|element| is_listed_file(element))
        .map(|element| element.name.trim().to_lowercase())
        .collect();
    names.sort();
    names
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

fn lists_a_new_file(elements: &[Element], destination: &Destination) -> bool {
    listed_files(elements) != destination.listed_before && lists_a_file(elements, &destination.names)
}

fn lists_a_file(elements: &[Element], names: &[String]) -> bool {
    elements
        .iter()
        .filter(|element| is_listed_file(element))
        .any(|element| names.iter().any(|name| shows_file(&element.name, name)))
}

fn is_listed_file(element: &Element) -> bool {
    element.is_visible() && matches!(element.role, Role::ListItem | Role::DataItem)
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

    fn destination(listed_before: &[&str]) -> Destination {
        Destination {
            folder: "C:\\Data".to_string(),
            names: vec!["first.txt".to_string(), "second.txt".to_string()],
            listed_before: listed_before.iter().map(|name| (*name).to_string()).collect(),
        }
    }

    fn dialog(extra: Vec<Element>) -> Snapshot {
        let mut typed = with_id(element(2, 1, Role::Edit, "File name:"), "1148");
        typed.value = Some("C:\\Data".to_string());
        let mut elements = vec![element(1, 0, Role::Window, "Open"), typed];
        elements.extend(extra);
        snapshot("Open", elements)
    }

    fn ready(extra: Vec<Element>, listed_before: &[&str]) -> bool {
        folder_ready(&dialog(extra), NAME_BOX, &destination(listed_before))
    }

    #[test]
    fn the_address_bar_naming_the_folder_confirms_it() {
        let address = element(3, 1, Role::ToolBar, "Address: C:\\Data\\");
        assert!(ready(vec![address], &[]));
        let elsewhere = element(3, 1, Role::ToolBar, "Address: C:\\Users\\me\\Documents");
        assert!(!ready(vec![elsewhere], &[]));
    }

    #[test]
    fn the_typed_name_box_alone_proves_nothing() {
        let mut echoed = element(2, 1, Role::Edit, "C:\\Data");
        echoed.id = NAME_BOX;
        assert!(!ready(vec![echoed], &[]));
        assert!(!ready(Vec::new(), &[]));
    }

    #[test]
    fn a_listed_file_confirms_the_folder_once_the_listing_has_changed() {
        assert!(ready(vec![element(4, 2, Role::ListItem, "second.txt")], &["notes.txt"]));
        assert!(ready(vec![element(4, 2, Role::ListItem, "First")], &[]));
        assert!(!ready(vec![element(4, 2, Role::Button, "first.txt")], &[]));
    }

    #[test]
    fn same_named_files_still_showing_from_the_old_folder_prove_nothing() {
        let old_folder = vec![element(4, 2, Role::ListItem, "First.txt"), element(5, 2, Role::ListItem, "notes.txt")];
        let before = listed_files(&old_folder);
        assert_eq!(before, ["first.txt", "notes.txt"]);
        let names: Vec<&str> = before.iter().map(String::as_str).collect();
        assert!(!ready(old_folder, &names));
    }
}
