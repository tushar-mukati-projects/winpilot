use windows::core::Result;

use crate::dialogs::Choice;
use crate::shortcuts::Shortcut;

pub const COLOURS: [&str; 3] = ["Red", "Green", "Blue"];
pub const ITEM_COUNT: usize = 30;
pub const APPEARED_CLICKED: &str = "Appeared clicked";
const PATH_SEPARATOR: &str = " | ";

pub fn clicks(count: u32) -> String {
    format!("Clicks: {count}")
}

pub fn feature(enabled: bool) -> &'static str {
    if enabled { "Feature: on" } else { "Feature: off" }
}

pub fn colour(index: Option<usize>) -> String {
    let name = index.and_then(|index| COLOURS.get(index)).copied().unwrap_or("none");
    format!("Colour: {name}")
}

pub fn item_label(number: usize) -> String {
    format!("Item {number}")
}

pub fn selected_item(index: Option<usize>) -> String {
    match index.filter(|index| *index < ITEM_COUNT) {
        Some(index) => format!("Selected: {}", item_label(index + 1)),
        None => "Selected: none".to_owned(),
    }
}

pub fn opened(outcome: &Result<Choice<Vec<String>>>) -> String {
    match outcome {
        Ok(Choice::Chosen(paths)) => format!("Opened: {}", paths.join(PATH_SEPARATOR)),
        Ok(Choice::Cancelled) => "Opened: cancelled".to_owned(),
        Err(error) => format!("Opened: error {}", error.code()),
    }
}

pub fn saved(outcome: &Result<Choice<String>>) -> String {
    match outcome {
        Ok(Choice::Chosen(path)) => format!("Saved: {path}"),
        Ok(Choice::Cancelled) => "Saved: cancelled".to_owned(),
        Err(error) => format!("Saved: error {}", error.code()),
    }
}

pub fn last_key(shortcut: Shortcut) -> String {
    format!("Last key: {}", shortcut.label())
}

#[cfg(test)]
mod tests {
    use windows::Win32::Foundation::E_ACCESSDENIED;
    use windows::core::Error;

    use super::*;

    #[test]
    fn click_counter_text() {
        assert_eq!(clicks(0), "Clicks: 0");
        assert_eq!(clicks(12), "Clicks: 12");
    }

    #[test]
    fn feature_text() {
        assert_eq!(feature(false), "Feature: off");
        assert_eq!(feature(true), "Feature: on");
    }

    #[test]
    fn colour_text_covers_every_option_and_out_of_range() {
        assert_eq!(colour(None), "Colour: none");
        assert_eq!(colour(Some(0)), "Colour: Red");
        assert_eq!(colour(Some(1)), "Colour: Green");
        assert_eq!(colour(Some(2)), "Colour: Blue");
        assert_eq!(colour(Some(3)), "Colour: none");
    }

    #[test]
    fn item_text_is_one_based() {
        assert_eq!(item_label(1), "Item 1");
        assert_eq!(selected_item(None), "Selected: none");
        assert_eq!(selected_item(Some(0)), "Selected: Item 1");
        assert_eq!(selected_item(Some(29)), "Selected: Item 30");
        assert_eq!(selected_item(Some(30)), "Selected: none");
    }

    #[test]
    fn opened_text_joins_paths() {
        let paths = vec![r"C:\a.txt".to_owned(), r"C:\b c.txt".to_owned()];
        assert_eq!(opened(&Ok(Choice::Chosen(paths))), r"Opened: C:\a.txt | C:\b c.txt");
        assert_eq!(opened(&Ok(Choice::Cancelled)), "Opened: cancelled");
        assert_eq!(opened(&Err(Error::from_hresult(E_ACCESSDENIED))), "Opened: error 0x80070005");
    }

    #[test]
    fn saved_text_reports_path_or_outcome() {
        assert_eq!(saved(&Ok(Choice::Chosen(r"D:\report.txt".to_owned()))), r"Saved: D:\report.txt");
        assert_eq!(saved(&Ok(Choice::Cancelled)), "Saved: cancelled");
        assert_eq!(saved(&Err(Error::from_hresult(E_ACCESSDENIED))), "Saved: error 0x80070005");
    }

    #[test]
    fn last_key_text() {
        assert_eq!(last_key(Shortcut::F5), "Last key: F5");
        assert_eq!(last_key(Shortcut::CtrlK), "Last key: Ctrl+K");
    }
}
