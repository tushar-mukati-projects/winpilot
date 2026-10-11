use deckhand_core::{Element, ElementRef, Role, Snapshot, WindowInfo, protocol::DialogKind};

const FILE_NAME_ID: &str = "1148";
const SAVE_NAME_ID: &str = "1001";
const LEGACY_NAME_ID: &str = "1152";
const CONFIRM_ID: &str = "1";
const CANCEL_ID: &str = "2";
const CONFIRM_NAMES: [&str; 2] = ["open", "save"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDialog {
    pub window: WindowInfo,
    pub kind: DialogKind,
    pub name_box: ElementRef,
    pub confirm: ElementRef,
    pub cancel: Option<ElementRef>,
}

pub fn recognise(snapshot: &Snapshot) -> Option<FileDialog> {
    let name_box = name_box(snapshot)?;
    let confirm = confirm_button(snapshot)?;
    let cancel = button_with_id(snapshot, CANCEL_ID);
    Some(FileDialog { window: snapshot.window.clone(), kind: kind(snapshot), name_box, confirm, cancel })
}

fn name_box(snapshot: &Snapshot) -> Option<ElementRef> {
    combo_edit(snapshot)
        .or_else(|| edit_with_id(snapshot, FILE_NAME_ID))
        .or_else(|| edit_with_id(snapshot, SAVE_NAME_ID))
        .or_else(|| edit_with_id(snapshot, LEGACY_NAME_ID))
}

fn combo_edit(snapshot: &Snapshot) -> Option<ElementRef> {
    let combo = snapshot.elements.iter().position(is_file_name_combo)?;
    snapshot.descendants(combo).iter().find(|element| element.role == Role::Edit).map(|element| element.id)
}

fn is_file_name_combo(element: &Element) -> bool {
    element.role == Role::Combobox && element.automation_id == FILE_NAME_ID
}

fn edit_with_id(snapshot: &Snapshot, id: &str) -> Option<ElementRef> {
    snapshot.elements.iter().find(|element| element.role == Role::Edit && element.automation_id == id).map(|e| e.id)
}

fn confirm_button(snapshot: &Snapshot) -> Option<ElementRef> {
    let candidates: Vec<&Element> = snapshot
        .elements
        .iter()
        .filter(|element| {
            matches!(element.role, Role::Button | Role::SplitButton) && element.automation_id == CONFIRM_ID
        })
        .collect();
    let named = candidates.iter().find(|element| CONFIRM_NAMES.contains(&plain_name(&element.name).as_str()));
    named.or_else(|| candidates.first()).map(|element| element.id)
}

fn button_with_id(snapshot: &Snapshot, id: &str) -> Option<ElementRef> {
    let button =
        |element: &&Element| matches!(element.role, Role::Button | Role::SplitButton) && element.automation_id == id;
    snapshot.elements.iter().find(button).map(|element| element.id)
}

fn kind(snapshot: &Snapshot) -> DialogKind {
    let has_combo = snapshot.elements.iter().any(is_file_name_combo);
    let has_save_edit = edit_with_id(snapshot, SAVE_NAME_ID).is_some();
    let titled_save = snapshot.window.title.to_lowercase().contains("save");
    if (!has_combo && has_save_edit) || titled_save { DialogKind::Save } else { DialogKind::Open }
}

pub fn plain_name(name: &str) -> String {
    name.replace('&', "").trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixture::{element, snapshot, with_id};

    fn open_dialog(title: &str) -> Snapshot {
        snapshot(
            title,
            vec![
                element(1, 0, Role::Window, title),
                with_id(element(2, 1, Role::Combobox, "File name:"), "1148"),
                with_id(element(3, 2, Role::Edit, "File name:"), "1148"),
                with_id(element(4, 1, Role::SplitButton, "&Open"), "1"),
                with_id(element(5, 1, Role::Button, "Cancel"), "2"),
            ],
        )
    }

    #[test]
    fn recognises_a_common_open_dialog() {
        let dialog = recognise(&open_dialog("Open")).unwrap();
        assert_eq!((dialog.kind, dialog.name_box, dialog.confirm), (DialogKind::Open, ElementRef(3), ElementRef(4)));
        assert_eq!(dialog.cancel, Some(ElementRef(5)));
    }

    #[test]
    fn a_save_title_makes_a_1148_dialog_a_save_dialog() {
        assert_eq!(recognise(&open_dialog("Save As")).unwrap().kind, DialogKind::Save);
    }

    #[test]
    fn a_1001_edit_without_the_combo_is_a_save_dialog_in_any_language() {
        let tree = snapshot(
            "Speichern unter",
            vec![
                with_id(element(1, 0, Role::Pane, ""), "FileNameControlHost"),
                with_id(element(2, 1, Role::Edit, "Dateiname:"), "1001"),
                with_id(element(3, 0, Role::Button, "Speichern"), "1"),
            ],
        );
        let dialog = recognise(&tree).unwrap();
        assert_eq!((dialog.kind, dialog.name_box, dialog.confirm), (DialogKind::Save, ElementRef(2), ElementRef(3)));
    }

    #[test]
    fn localised_open_dialogs_are_found_by_id() {
        let mut tree = open_dialog("Öffnen");
        tree.elements[3].name = "Ö&ffnen".to_string();
        let dialog = recognise(&tree).unwrap();
        assert_eq!((dialog.kind, dialog.confirm), (DialogKind::Open, ElementRef(4)));
    }

    #[test]
    fn names_break_ties_between_confirm_buttons() {
        let mut tree = open_dialog("Open");
        tree.elements.insert(3, with_id(element(9, 1, Role::Button, "Help"), "1"));
        assert_eq!(recognise(&tree).unwrap().confirm, ElementRef(4));
    }

    #[test]
    fn legacy_dialogs_use_1152() {
        let tree = snapshot(
            "Open",
            vec![
                with_id(element(1, 0, Role::Edit, "File name:"), "1152"),
                with_id(element(2, 0, Role::Button, "Open"), "1"),
            ],
        );
        assert_eq!(recognise(&tree).unwrap().name_box, ElementRef(1));
    }

    #[test]
    fn other_dialogs_are_not_file_dialogs() {
        let message = snapshot(
            "Notepad",
            vec![element(1, 0, Role::Text, "Save changes?"), with_id(element(2, 0, Role::Button, "Save"), "6")],
        );
        assert_eq!(recognise(&message), None);
        let mut no_button = open_dialog("Open");
        no_button.elements.remove(3);
        assert_eq!(recognise(&no_button), None);
    }

    #[test]
    fn plain_names_drop_accelerators() {
        assert_eq!(plain_name(" &Yes "), "yes");
        assert_eq!(plain_name("O&K"), "ok");
    }
}
