use deckhand_core::{ElementRef, Role, Snapshot, protocol::DialogKind};

use super::{dialog::plain_name, selection::Requested};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Button {
    ids: &'static [&'static str],
    name: &'static str,
}

const YES: Button = Button { ids: &["6", "CommandButton_6"], name: "yes" };
const NO: Button = Button { ids: &["7", "CommandButton_7"], name: "no" };
const CANCEL: Button = Button { ids: &["2", "CommandButton_2"], name: "cancel" };
const OK: Button = Button { ids: &["1", "CommandButton_1"], name: "ok" };
const CLOSE: Button = Button { ids: &["8", "CommandButton_8"], name: "close" };
const DISMISSERS: [Button; 4] = [CANCEL, NO, OK, CLOSE];
const NO_TEXT: &str = "the dialog showed a message without text";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Prompt {
    Replace { yes: ElementRef, no: ElementRef },
    Question { text: String, decline: Option<ElementRef> },
    Message { text: String, dismiss: Option<ElementRef> },
}

pub fn classify(snapshot: &Snapshot, kind: DialogKind, requested: &Requested) -> Prompt {
    let text = message_text(snapshot);
    let dismiss = DISMISSERS.iter().find_map(|wanted| button(snapshot, *wanted));
    let (Some(yes), Some(no)) = (button(snapshot, YES), button(snapshot, NO)) else {
        return Prompt::Message { text, dismiss };
    };
    if kind == DialogKind::Save && asks_to_replace(&text, requested) {
        return Prompt::Replace { yes, no };
    }
    Prompt::Question { text, decline: dismiss }
}

fn asks_to_replace(text: &str, requested: &Requested) -> bool {
    let name = requested.name.trim().to_lowercase();
    requested.existed || (!name.is_empty() && text.to_lowercase().contains(&name))
}

fn button(snapshot: &Snapshot, wanted: Button) -> Option<ElementRef> {
    let buttons = || snapshot.elements.iter().filter(|element| element.role == Role::Button);
    buttons()
        .find(|element| wanted.ids.contains(&element.automation_id.as_str()))
        .or_else(|| buttons().find(|element| plain_name(&element.name) == wanted.name))
        .map(|element| element.id)
}

fn message_text(snapshot: &Snapshot) -> String {
    let lines: Vec<&str> = snapshot
        .elements
        .iter()
        .filter(|element| element.role == Role::Text)
        .map(|element| element.name.trim())
        .filter(|name| !name.is_empty())
        .collect();
    if lines.is_empty() { NO_TEXT.to_string() } else { lines.join(" ") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_fixture::{element, snapshot, with_id};

    fn yes_no_box(message: &str, yes: (&str, &str), no: (&str, &str)) -> Snapshot {
        snapshot(
            "Confirm Save As",
            vec![
                element(1, 0, Role::Text, message),
                element(2, 0, Role::Text, "Do you want to replace it?"),
                with_id(element(3, 0, Role::Button, yes.1), yes.0),
                with_id(element(4, 0, Role::Button, no.1), no.0),
            ],
        )
    }

    fn requested(name: &str, existed: bool) -> Requested {
        Requested { name: name.to_string(), existed }
    }

    fn replace() -> Prompt {
        Prompt::Replace { yes: ElementRef(3), no: ElementRef(4) }
    }

    #[test]
    fn a_box_naming_the_requested_file_asks_to_replace_it() {
        let classic = yes_no_box("report.txt already exists.", ("6", "&Yes"), ("7", "&No"));
        assert_eq!(classify(&classic, DialogKind::Save, &requested("Report.txt", false)), replace());
        let task_dialog =
            yes_no_box("report.txt existiert bereits.", ("CommandButton_6", "Ja"), ("CommandButton_7", "Nein"));
        assert_eq!(classify(&task_dialog, DialogKind::Save, &requested("report.txt", false)), replace());
        let unnumbered = yes_no_box("report.txt already exists.", ("", "&Yes"), ("", "No"));
        assert_eq!(classify(&unnumbered, DialogKind::Save, &requested("report.txt", false)), replace());
    }

    #[test]
    fn an_existing_file_makes_any_yes_no_box_a_replace_prompt() {
        let worded = yes_no_box("The file already exists.", ("6", "&Yes"), ("7", "&No"));
        assert_eq!(classify(&worded, DialogKind::Save, &requested("report.txt", true)), replace());
    }

    #[test]
    fn other_yes_no_boxes_are_unknown_questions_declined_with_no() {
        let other = yes_no_box("Change the file type to .csv?", ("6", "&Yes"), ("7", "&No"));
        let prompt = classify(&other, DialogKind::Save, &requested("report.txt", false));
        let text = "Change the file type to .csv? Do you want to replace it?".to_string();
        assert_eq!(prompt, Prompt::Question { text, decline: Some(ElementRef(4)) });
    }

    #[test]
    fn the_same_box_in_an_open_dialog_is_a_question_too() {
        let boxed = yes_no_box("report.txt already exists.", ("6", "&Yes"), ("7", "&No"));
        let prompt = classify(&boxed, DialogKind::Open, &requested("report.txt", true));
        assert!(matches!(prompt, Prompt::Question { decline: Some(ElementRef(4)), .. }), "{prompt:?}");
    }

    #[test]
    fn cancel_is_preferred_for_declining() {
        let mut three = yes_no_box("Save changes to report.txt first?", ("6", "&Yes"), ("7", "&No"));
        three.elements.push(with_id(element(5, 0, Role::Button, "Cancel"), "2"));
        let prompt = classify(&three, DialogKind::Save, &requested("other.txt", false));
        assert!(matches!(prompt, Prompt::Question { decline: Some(ElementRef(5)), .. }), "{prompt:?}");
    }

    #[test]
    fn an_error_box_is_read_and_dismissed_with_ok() {
        let error = snapshot(
            "Open",
            vec![
                element(1, 0, Role::Image, "Warning"),
                element(2, 0, Role::Text, "File not found."),
                element(3, 0, Role::Text, " Check the file name and try again. "),
                with_id(element(4, 0, Role::Button, "OK"), "2"),
            ],
        );
        let text = "File not found. Check the file name and try again.".to_string();
        assert_eq!(
            classify(&error, DialogKind::Open, &requested("a.txt", false)),
            Prompt::Message { text, dismiss: Some(ElementRef(4)) }
        );
    }

    #[test]
    fn ok_with_id_one_or_by_name_still_dismisses() {
        let id_one =
            snapshot("x", vec![element(1, 0, Role::Text, "Oops"), with_id(element(2, 0, Role::Button, "OK"), "1")]);
        let prompt = classify(&id_one, DialogKind::Save, &requested("a.txt", false));
        assert!(matches!(prompt, Prompt::Message { dismiss: Some(ElementRef(2)), .. }));
        let named = snapshot("x", vec![element(1, 0, Role::Button, "O&K")]);
        assert_eq!(
            classify(&named, DialogKind::Open, &requested("a.txt", false)),
            Prompt::Message { text: NO_TEXT.to_string(), dismiss: Some(ElementRef(1)) }
        );
    }

    #[test]
    fn a_box_without_buttons_cannot_be_dismissed() {
        let bare = snapshot("x", vec![element(1, 0, Role::Text, "Working…")]);
        assert_eq!(
            classify(&bare, DialogKind::Open, &requested("a.txt", false)),
            Prompt::Message { text: "Working…".to_string(), dismiss: None }
        );
    }
}
