use winpilot_core::{WindowInfo, text_limit::capped};

use crate::window_classes::FRAME_HOST_PROCESS;

const LONGEST: usize = 32;
const TITLE_SEPARATORS: [&str; 3] = [" - ", " \u{2013} ", " \u{2014} "];
const UNNAMED: &str = "an app";

pub(crate) fn app_label(window: &WindowInfo) -> String {
    let named = title_app(&window.title)
        .or_else(|| hosted_title(window))
        .or_else(|| process_label(&window.process))
        .unwrap_or_else(|| UNNAMED.to_string());
    capped(&named, LONGEST)
}

fn title_app(title: &str) -> Option<String> {
    let visible: String = title.chars().filter(|letter| !is_invisible(*letter)).collect();
    let tail = TITLE_SEPARATORS
        .iter()
        .filter_map(|separator| visible.rsplit_once(separator).map(|(_, tail)| tail.trim()))
        .min_by_key(|tail| tail.len())?;
    (!tail.is_empty()).then(|| tail.to_string())
}

fn hosted_title(window: &WindowInfo) -> Option<String> {
    let hosted = window.process.trim().eq_ignore_ascii_case(FRAME_HOST_PROCESS);
    let title = window.title.trim();
    (hosted && !title.is_empty()).then(|| title.to_string())
}

fn process_label(process: &str) -> Option<String> {
    let trimmed = process.trim();
    let stem = match trimmed.len().checked_sub(4).and_then(|cut| trimmed.get(cut..).map(|tail| (cut, tail))) {
        Some((cut, tail)) if tail.eq_ignore_ascii_case(".exe") => trimmed.get(..cut).unwrap_or(trimmed),
        _ => trimmed,
    };
    if stem.is_empty() {
        return None;
    }
    if stem.chars().any(char::is_uppercase) {
        return Some(stem.to_string());
    }
    let mut letters = stem.chars();
    letters.next().map(|first| first.to_uppercase().chain(letters).collect())
}

const fn is_invisible(letter: char) -> bool {
    matches!(letter, '\u{200B}'..='\u{200F}' | '\u{2060}' | '\u{FEFF}')
}

#[cfg(test)]
mod tests {
    use winpilot_core::Rect;

    use super::*;
    use crate::test_fixture::window;

    fn label(process: &str, title: &str) -> String {
        app_label(&window(1, process, title, Rect::new(0, 0, 10, 10)))
    }

    #[test]
    fn the_app_is_the_last_part_of_the_title() {
        assert_eq!(label("notepad.exe", "Untitled - Notepad"), "Notepad");
        assert_eq!(label("chrome.exe", "Inbox - Mail - Google Chrome"), "Google Chrome");
        assert_eq!(label("msedge.exe", "News \u{2014} Microsoft\u{200B} Edge"), "Microsoft Edge");
    }

    #[test]
    fn titles_without_an_app_part_fall_back_to_the_process() {
        assert_eq!(label("CapCut.exe", "CapCut"), "CapCut");
        assert_eq!(label("explorer.exe", "Downloads"), "Explorer");
        assert_eq!(label("slack.EXE", "Slack | general"), "Slack");
        assert_eq!(label("", ""), "an app");
    }

    #[test]
    fn apps_hosted_by_the_frame_host_use_their_title() {
        assert_eq!(label(FRAME_HOST_PROCESS, "Calculator"), "Calculator");
        assert_eq!(label(FRAME_HOST_PROCESS, ""), "ApplicationFrameHost");
    }

    #[test]
    fn long_names_are_cut_with_an_ellipsis() {
        let cut = label("a.exe", "doc - An Application With A Remarkably Long Product Name");
        assert_eq!(cut.chars().count(), LONGEST);
        assert!(cut.ends_with('\u{2026}'));
    }
}
