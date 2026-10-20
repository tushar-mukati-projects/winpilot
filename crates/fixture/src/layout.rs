use crate::dpi::scale;
use crate::ids::{
    APPEARED_BUTTON, APPEARED_STATUS, CLICK_BUTTON, CLICK_STATUS, COLOUR_COMBO, COLOUR_LABEL, COLOUR_STATUS,
    FEATURE_CHECKBOX, FEATURE_STATUS, ITEM_LABEL, ITEM_LIST, ITEM_STATUS, KEY_STATUS, NAME_EDIT, NAME_LABEL,
    NOTES_EDIT, NOTES_LABEL, OPEN_BUTTON, OPEN_STATUS, SAVE_BUTTON, SAVE_STATUS, SHOW_LATER_BUTTON,
};

pub const CLIENT_WIDTH: i32 = 624;
pub const CLIENT_HEIGHT: i32 = 644;

const LABEL_X: i32 = 16;
const LABEL_WIDTH: i32 = 72;
const FIELD_X: i32 = 96;
const FIELD_WIDTH: i32 = 320;
const BUTTON_WIDTH: i32 = 140;
const BUTTON_HEIGHT: i32 = 28;
const STATUS_X: i32 = 252;
const TEXT_HEIGHT: i32 = 20;
const TEXT_OFFSET: i32 = 4;
const NAME_HEIGHT: i32 = 24;
const NOTES_HEIGHT: i32 = 100;
const CHECKBOX_WIDTH: i32 = 150;
const COLOUR_DROP_HEIGHT: i32 = 200;
const ITEM_LIST_HEIGHT: i32 = 140;
const STATUS_GAP: i32 = 16;

const NAME_ROW: i32 = 20;
const NOTES_ROW: i32 = 60;
const CLICK_ROW: i32 = 180;
const FEATURE_ROW: i32 = 224;
const COLOUR_ROW: i32 = 268;
const ITEM_ROW: i32 = 312;
const OPEN_ROW: i32 = 472;
const SAVE_ROW: i32 = 516;
const LATER_ROW: i32 = 560;
const KEY_ROW: i32 = 604;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlKind {
    Label,
    Status,
    SingleLineEdit,
    MultiLineEdit,
    PushButton,
    CheckBox,
    DropDownList,
    ListBox,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Bounds {
    const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self { x, y, width, height }
    }

    pub fn scaled(self, dpi: u32) -> Self {
        Self::new(scale(self.x, dpi), scale(self.y, dpi), scale(self.width, dpi), scale(self.height, dpi))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlSpec {
    pub id: i32,
    pub kind: ControlKind,
    pub text: &'static str,
    pub bounds: Bounds,
    pub initially_visible: bool,
}

const fn spec(id: i32, kind: ControlKind, text: &'static str, bounds: Bounds) -> ControlSpec {
    ControlSpec { id, kind, text, bounds, initially_visible: true }
}

const fn label(id: i32, text: &'static str, row: i32) -> ControlSpec {
    spec(id, ControlKind::Label, text, Bounds::new(LABEL_X, row + TEXT_OFFSET, LABEL_WIDTH, TEXT_HEIGHT))
}

const fn status(id: i32, text: &'static str, x: i32, row: i32) -> ControlSpec {
    let width = CLIENT_WIDTH - LABEL_X - x;
    spec(id, ControlKind::Status, text, Bounds::new(x, row + TEXT_OFFSET, width, TEXT_HEIGHT))
}

const fn button(id: i32, text: &'static str, x: i32, row: i32) -> ControlSpec {
    spec(id, ControlKind::PushButton, text, Bounds::new(x, row, BUTTON_WIDTH, BUTTON_HEIGHT))
}

const fn field(id: i32, kind: ControlKind, row: i32, width: i32, height: i32) -> ControlSpec {
    spec(id, kind, "", Bounds::new(FIELD_X, row, width, height))
}

const fn hidden(control: ControlSpec) -> ControlSpec {
    ControlSpec { initially_visible: false, ..control }
}

pub const CONTROLS: [ControlSpec; 22] = [
    label(NAME_LABEL, "Name", NAME_ROW),
    field(NAME_EDIT, ControlKind::SingleLineEdit, NAME_ROW, FIELD_WIDTH, NAME_HEIGHT),
    label(NOTES_LABEL, "Notes", NOTES_ROW),
    field(NOTES_EDIT, ControlKind::MultiLineEdit, NOTES_ROW, FIELD_WIDTH, NOTES_HEIGHT),
    button(CLICK_BUTTON, "Click me", FIELD_X, CLICK_ROW),
    status(CLICK_STATUS, "Clicks: 0", STATUS_X, CLICK_ROW),
    spec(
        FEATURE_CHECKBOX,
        ControlKind::CheckBox,
        "Enable feature",
        Bounds::new(FIELD_X, FEATURE_ROW, CHECKBOX_WIDTH, BUTTON_HEIGHT),
    ),
    status(FEATURE_STATUS, "Feature: off", STATUS_X, FEATURE_ROW),
    label(COLOUR_LABEL, "Colour", COLOUR_ROW),
    field(COLOUR_COMBO, ControlKind::DropDownList, COLOUR_ROW, BUTTON_WIDTH, COLOUR_DROP_HEIGHT),
    status(COLOUR_STATUS, "Colour: none", STATUS_X, COLOUR_ROW),
    label(ITEM_LABEL, "Items", ITEM_ROW),
    field(ITEM_LIST, ControlKind::ListBox, ITEM_ROW, BUTTON_WIDTH, ITEM_LIST_HEIGHT),
    status(ITEM_STATUS, "Selected: none", STATUS_X, ITEM_ROW),
    button(OPEN_BUTTON, "Open file\u{2026}", FIELD_X, OPEN_ROW),
    status(OPEN_STATUS, "Opened: none", STATUS_X, OPEN_ROW),
    button(SAVE_BUTTON, "Save file\u{2026}", FIELD_X, SAVE_ROW),
    status(SAVE_STATUS, "Saved: none", STATUS_X, SAVE_ROW),
    button(SHOW_LATER_BUTTON, "Show later", FIELD_X, LATER_ROW),
    hidden(button(APPEARED_BUTTON, "Appeared", STATUS_X, LATER_ROW)),
    status(APPEARED_STATUS, "", STATUS_X + BUTTON_WIDTH + STATUS_GAP, LATER_ROW),
    status(KEY_STATUS, "Last key: none", FIELD_X, KEY_ROW),
];

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::text;

    fn find(id: i32) -> ControlSpec {
        *CONTROLS.iter().find(|control| control.id == id).unwrap()
    }

    fn position(id: i32) -> usize {
        CONTROLS.iter().position(|control| control.id == id).unwrap()
    }

    #[test]
    fn ids_are_unique() {
        let ids: HashSet<i32> = CONTROLS.iter().map(|control| control.id).collect();
        assert_eq!(ids.len(), CONTROLS.len());
    }

    #[test]
    fn every_contract_id_exists() {
        for id in (2001..=2017).chain([KEY_STATUS]) {
            assert!(CONTROLS.iter().any(|control| control.id == id), "missing {id}");
        }
    }

    #[test]
    fn controls_fit_inside_the_client_area() {
        for control in CONTROLS {
            let bounds = control.bounds;
            assert!(bounds.x >= 0 && bounds.y >= 0, "{}", control.id);
            assert!(bounds.x + bounds.width <= CLIENT_WIDTH, "{}", control.id);
            assert!(bounds.y + bounds.height <= CLIENT_HEIGHT, "{}", control.id);
        }
    }

    #[test]
    fn initial_texts_match_the_status_formatters() {
        assert_eq!(find(CLICK_STATUS).text, text::clicks(0));
        assert_eq!(find(FEATURE_STATUS).text, text::feature(false));
        assert_eq!(find(COLOUR_STATUS).text, text::colour(None));
        assert_eq!(find(ITEM_STATUS).text, text::selected_item(None));
        assert_eq!(find(APPEARED_STATUS).text, "");
    }

    #[test]
    fn button_texts_follow_the_contract() {
        assert_eq!(find(OPEN_BUTTON).text, "Open file…");
        assert_eq!(find(SAVE_BUTTON).text, "Save file…");
    }

    #[test]
    fn only_the_appeared_button_starts_hidden() {
        let hidden: Vec<i32> = CONTROLS.iter().filter(|c| !c.initially_visible).map(|c| c.id).collect();
        assert_eq!(hidden, [APPEARED_BUTTON]);
    }

    #[test]
    fn labels_directly_precede_their_fields() {
        for (label, field) in
            [(NAME_LABEL, NAME_EDIT), (NOTES_LABEL, NOTES_EDIT), (COLOUR_LABEL, COLOUR_COMBO), (ITEM_LABEL, ITEM_LIST)]
        {
            assert_eq!(position(label) + 1, position(field), "{label}");
            assert_eq!(find(label).kind, ControlKind::Label);
        }
    }

    #[test]
    fn scaling_preserves_base_layout() {
        let bounds = find(NOTES_EDIT).bounds;
        assert_eq!(bounds.scaled(96), bounds);
        assert_eq!(bounds.scaled(192), Bounds::new(192, 120, 640, 200));
    }
}
