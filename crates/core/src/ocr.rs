use serde::{Deserialize, Serialize};

use crate::{
    element::{Element, Snapshot},
    error::{Error, ErrorCode, Result},
    geometry::Rect,
    matching::fold,
    role::Role,
};

const LISTED_CANDIDATES: usize = 5;
pub(crate) const NO_UIA_NOTE: &str = "this app exposes no UI Automation; showing OCR text — target it with ocr:\"…\"";
const BLANK_CONTAINERS: [Role; 5] = [Role::Pane, Role::Group, Role::Custom, Role::Window, Role::Unknown];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrWord {
    pub text: String,
    pub rect: Rect,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OcrLine {
    pub text: String,
    pub rect: Rect,
    #[serde(default)]
    pub words: Vec<OcrWord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcrMatch {
    pub text: String,
    pub rect: Rect,
    pub(crate) exact: bool,
}

pub fn find_matches(lines: &[OcrLine], query: &str) -> Vec<OcrMatch> {
    let wanted = fold(query);
    if wanted.is_empty() {
        return Vec::new();
    }
    let found: Vec<OcrMatch> = lines.iter().flat_map(|line| spans_in(line, &wanted)).collect();
    if found.iter().any(|candidate| candidate.exact) {
        return found.into_iter().filter(|candidate| candidate.exact).collect();
    }
    found
}

pub fn pick_match(matches: Vec<OcrMatch>, query: &str, nth: Option<u32>) -> Result<OcrMatch> {
    let count = matches.len();
    if let Some(position) = nth {
        let index = usize::try_from(position).unwrap_or(usize::MAX).saturating_sub(1);
        return matches.into_iter().nth(index).ok_or_else(|| too_few(query, position, count));
    }
    let mut matches = matches.into_iter();
    match (matches.next(), matches.len()) {
        (None, _) => Err(nothing_matches(query)),
        (Some(only), 0) => Ok(only),
        (Some(first), _) => Err(ambiguous(query, &std::iter::once(first).chain(matches).collect::<Vec<_>>())),
    }
}

pub fn exposes_no_uia(snapshot: &Snapshot) -> bool {
    let Some((_, inside)) = snapshot.elements.split_first() else { return true };
    let mut title_bar: Option<u16> = None;
    inside.iter().all(|element| {
        if title_bar.is_some_and(|depth| element.depth > depth) {
            return true;
        }
        title_bar = (element.role == Role::TitleBar).then_some(element.depth);
        title_bar.is_some() || is_blank_container(element)
    })
}

fn is_blank_container(element: &Element) -> bool {
    let blank = |text: &str| text.trim().is_empty();
    BLANK_CONTAINERS.contains(&element.role) && blank(&element.name) && element.value.as_deref().is_none_or(blank)
}

pub fn line_with_coords(line: &OcrLine) -> String {
    format!("{} {}", line.text.trim(), coords(line.rect))
}

pub fn coords(rect: Rect) -> String {
    let Rect { x, y, width, height } = rect;
    format!("@{x},{y},{width}x{height}")
}

fn spans_in(line: &OcrLine, wanted: &str) -> Vec<OcrMatch> {
    let whole = [OcrWord { text: line.text.clone(), rect: line.rect }];
    let words: &[OcrWord] = if line.words.is_empty() { &whole } else { &line.words };
    let folded: Vec<String> = words.iter().map(|word| fold(&word.text)).collect();
    (0..words.len())
        .filter_map(|start| shortest_span_from(&folded, start, wanted))
        .filter(|&(start, end)| !joined(&folded, start + 1, end).contains(wanted))
        .map(|(start, end)| span_match(words, &folded, start, end, wanted))
        .collect()
}

fn shortest_span_from(folded: &[String], start: usize, wanted: &str) -> Option<(usize, usize)> {
    (start..folded.len()).find(|&end| joined(folded, start, end).contains(wanted)).map(|end| (start, end))
}

fn joined(folded: &[String], start: usize, end: usize) -> String {
    folded.get(start..=end).map(|words| words.join(" ")).unwrap_or_default()
}

fn span_match(words: &[OcrWord], folded: &[String], start: usize, end: usize, wanted: &str) -> OcrMatch {
    let span = &words[start..=end];
    let rect = span.iter().map(|word| word.rect).reduce(Rect::union).unwrap_or_default();
    let text = span.iter().map(|word| word.text.trim()).collect::<Vec<_>>().join(" ");
    OcrMatch { text, rect, exact: joined(folded, start, end) == wanted }
}

fn too_few(query: &str, position: u32, count: usize) -> Error {
    if count == 0 {
        return nothing_matches(query);
    }
    Error::not_found(format!("nth:{position} asks for more than the {count} places showing ocr:\"{query}\""))
}

fn nothing_matches(query: &str) -> Error {
    Error::not_found(format!("no text on screen matches ocr:\"{query}\""))
}

fn ambiguous(query: &str, matches: &[OcrMatch]) -> Error {
    let mut lines = vec![format!("{} places show ocr:\"{query}\"; add nth: or narrow the text", matches.len())];
    let listed = matches.iter().take(LISTED_CANDIDATES);
    lines.extend(listed.map(|candidate| format!("  \"{}\" {}", candidate.text, coords(candidate.rect))));
    if matches.len() > LISTED_CANDIDATES {
        lines.push("  …".to_string());
    }
    Error::new(ErrorCode::Ambiguous, lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(text: &str, x: i32) -> OcrWord {
        OcrWord { text: text.to_string(), rect: Rect::new(x, 100, 10 * text.chars().count() as i32, 20) }
    }

    fn line(words: &[&str], y: i32) -> OcrLine {
        let mut x = 0;
        let words: Vec<OcrWord> = words
            .iter()
            .map(|text| {
                let placed = OcrWord { rect: Rect::new(x, y, 10 * text.chars().count() as i32, 20), ..word(text, x) };
                x += placed.rect.width + 5;
                placed
            })
            .collect();
        let rect = words.iter().map(|word| word.rect).reduce(Rect::union).unwrap_or_default();
        OcrLine { text: words.iter().map(|word| word.text.as_str()).collect::<Vec<_>>().join(" "), rect, words }
    }

    fn texts(matches: &[OcrMatch]) -> Vec<&str> {
        matches.iter().map(|found| found.text.as_str()).collect()
    }

    #[test]
    fn a_match_covers_only_its_words_not_the_whole_line() {
        let lines = [line(&["File", "Edit", "Export", "video"], 10)];
        let found = find_matches(&lines, "export video");
        assert_eq!(texts(&found), ["Export video"]);
        assert!(found[0].exact);
        assert_eq!(found[0].rect, Rect::new(90, 10, 115, 20));
        assert_ne!(found[0].rect, lines[0].rect);
    }

    #[test]
    fn matching_ignores_case_and_spacing() {
        let lines = [line(&["Export", "Video"], 10)];
        assert_eq!(texts(&find_matches(&lines, "  EXPORT\n  video ")), ["Export Video"]);
    }

    #[test]
    fn exact_matches_beat_contains_matches() {
        let lines = [line(&["Export", "settings"], 10), line(&["Export"], 40), line(&["Exported", "files"], 70)];
        let found = find_matches(&lines, "export");
        assert_eq!(texts(&found), ["Export", "Export"]);
        assert!(found.iter().all(|found| found.exact));
        assert_eq!(found[1].rect.y, 40);
    }

    #[test]
    fn contains_matches_are_used_when_nothing_matches_exactly() {
        let lines = [line(&["Exported", "files"], 10), line(&["Re-export"], 40)];
        let found = find_matches(&lines, "export");
        assert_eq!(texts(&found), ["Exported", "Re-export"]);
        assert!(found.iter().all(|found| !found.exact));
    }

    #[test]
    fn a_query_can_start_and_end_inside_words() {
        let lines = [line(&["Export", "video", "now"], 10)];
        let found = find_matches(&lines, "port vid");
        assert_eq!(texts(&found), ["Export video"]);
    }

    #[test]
    fn repeated_words_in_one_line_are_separate_matches() {
        let lines = [line(&["Yes", "No", "Yes"], 10)];
        let found = find_matches(&lines, "yes");
        assert_eq!(found.len(), 2);
        assert!(found[0].rect.x < found[1].rect.x);
    }

    #[test]
    fn lines_without_word_boxes_match_as_one_word() {
        let bare = OcrLine { text: "Save project".to_string(), rect: Rect::new(5, 6, 70, 12), words: Vec::new() };
        let found = find_matches(&[bare], "project");
        assert_eq!(found, [OcrMatch { text: "Save project".to_string(), rect: Rect::new(5, 6, 70, 12), exact: false }]);
    }

    #[test]
    fn blank_queries_and_missing_text_find_nothing() {
        let lines = [line(&["Export"], 10)];
        assert!(find_matches(&lines, "   ").is_empty());
        assert!(find_matches(&lines, "import").is_empty());
        assert!(find_matches(&[], "export").is_empty());
    }

    fn candidate(text: &str, x: i32) -> OcrMatch {
        OcrMatch { text: text.to_string(), rect: Rect::new(x, 10, 40, 20), exact: true }
    }

    #[test]
    fn one_match_is_picked_and_none_is_not_found() {
        assert_eq!(pick_match(vec![candidate("Export", 5)], "Export", None).unwrap(), candidate("Export", 5));
        let error = pick_match(Vec::new(), "Export", None).unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
        assert_eq!(error.message, "no text on screen matches ocr:\"Export\"");
    }

    #[test]
    fn several_matches_are_ambiguous_and_listed_with_positions() {
        let many: Vec<OcrMatch> = (0..7).map(|index| candidate("Export", index * 50)).collect();
        let error = pick_match(many, "Export", None).unwrap_err();
        assert_eq!(error.code, ErrorCode::Ambiguous);
        let lines: Vec<&str> = error.message.lines().collect();
        assert_eq!(lines[0], "7 places show ocr:\"Export\"; add nth: or narrow the text");
        assert_eq!(lines[1], "  \"Export\" @0,10,40x20");
        assert_eq!(lines.len(), 7);
        assert_eq!(lines[6], "  …");
    }

    #[test]
    fn nth_picks_in_reading_order() {
        let both = vec![candidate("Export", 5), candidate("Export", 90)];
        assert_eq!(pick_match(both.clone(), "Export", Some(2)).unwrap().rect.x, 90);
        let error = pick_match(both, "Export", Some(3)).unwrap_err();
        assert_eq!(error.code, ErrorCode::NotFound);
        assert_eq!(error.message, "nth:3 asks for more than the 2 places showing ocr:\"Export\"");
        let none = pick_match(Vec::new(), "Export", Some(1)).unwrap_err();
        assert_eq!(none.message, "no text on screen matches ocr:\"Export\"");
    }

    fn tree(elements: Vec<Element>) -> Snapshot {
        crate::element::fixtures::snapshot(elements)
    }

    fn named(id: u32, depth: u16, role: Role, name: &str) -> Element {
        crate::element::fixtures::element(id, depth, role, name)
    }

    #[test]
    fn a_lone_window_or_unnamed_panes_expose_no_ui_automation() {
        assert!(exposes_no_uia(&tree(vec![named(1, 0, Role::Window, "CapCut")])));
        assert!(exposes_no_uia(&tree(Vec::new())));
        let panes = vec![
            named(1, 0, Role::Window, "Game"),
            named(2, 1, Role::Pane, ""),
            named(3, 2, Role::Pane, "  "),
            named(4, 2, Role::Custom, ""),
        ];
        assert!(exposes_no_uia(&tree(panes)));
    }

    #[test]
    fn the_title_bar_and_its_buttons_do_not_count_as_content() {
        let framed = vec![
            named(1, 0, Role::Window, "Canvas app"),
            named(2, 1, Role::TitleBar, ""),
            named(3, 2, Role::MenuBar, "System Menu Bar"),
            named(4, 3, Role::MenuItem, "System"),
            named(5, 2, Role::Button, "Minimize"),
            named(6, 2, Role::Button, "Close"),
            named(7, 1, Role::Pane, ""),
        ];
        assert!(exposes_no_uia(&tree(framed)));
    }

    #[test]
    fn any_named_or_interactive_element_is_real_ui_automation() {
        let button = vec![named(1, 0, Role::Window, "App"), named(2, 1, Role::Button, "OK")];
        assert!(!exposes_no_uia(&tree(button)));
        let named_pane = vec![named(1, 0, Role::Window, "App"), named(2, 1, Role::Pane, "Sidebar")];
        assert!(!exposes_no_uia(&tree(named_pane)));
        let mut valued = named(2, 1, Role::Pane, "");
        valued.value = Some("42".to_string());
        assert!(!exposes_no_uia(&tree(vec![named(1, 0, Role::Window, "App"), valued])));
        let after_title = vec![
            named(1, 0, Role::Window, "App"),
            named(2, 1, Role::TitleBar, ""),
            named(3, 2, Role::Button, "Close"),
            named(4, 1, Role::Edit, ""),
        ];
        assert!(!exposes_no_uia(&tree(after_title)));
    }

    #[test]
    fn lines_print_with_screen_coordinates() {
        let line = OcrLine { text: " Export video ".to_string(), rect: Rect::new(-5, 20, 80, 14), words: Vec::new() };
        assert_eq!(line_with_coords(&line), "Export video @-5,20,80x14");
    }
}
