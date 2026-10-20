mod display;
mod hint;
mod lexer;
mod rank;
mod term;

use std::str::FromStr;

use crate::{
    element::{Element, Snapshot, WindowInfo},
    error::{Error, ErrorCode, Result},
    format::element_line,
};

pub use hint::SearchHint;
use lexer::{Piece, build_step, lex, term_builder};
use rank::{Rank, rank};
use term::Term;

#[derive(Debug, Clone, PartialEq, Eq)]
struct SelectorStep {
    terms: Vec<Term>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selector {
    steps: Vec<SelectorStep>,
}

const FOCUSED: &str = "focused";
const MAX_LISTED_CANDIDATES: usize = 5;
const OCR_ALONE: &str = "ocr: takes one text and combines only with window:, process: and nth:";

impl FromStr for Selector {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self> {
        let pieces = lex(text)?;
        if pieces.is_empty() {
            return Err(Error::invalid("empty selector"));
        }
        let steps = pieces.split(|piece| *piece == Piece::Chain).map(|group| build_step(group, text));
        let selector = Self { steps: steps.collect::<Result<_>>()? };
        selector.check_ocr()?;
        Ok(selector)
    }
}

impl Selector {
    pub fn has_window_terms(&self) -> bool {
        self.terms().any(Term::is_window_term)
    }

    pub fn has_element_terms(&self) -> bool {
        self.steps.iter().any(SelectorStep::has_element_terms)
    }

    pub fn matches_window(&self, window: &WindowInfo) -> bool {
        self.terms().all(|term| term.matches_window(window))
    }

    pub fn find<'a>(&self, snapshot: &'a Snapshot) -> Vec<&'a Element> {
        if !self.has_element_terms() || !self.matches_window(&snapshot.window) {
            return Vec::new();
        }
        let mut steps = self.steps.iter().filter(|step| step.filters_elements());
        let Some(first) = steps.next() else { return Vec::new() };
        let elements = &snapshot.elements;
        let mut matched = first.select(elements, 0..elements.len());
        for step in steps {
            matched = step.select(elements, descendant_indices(snapshot, &matched));
        }
        matched.into_iter().filter_map(|index| elements.get(index)).collect()
    }

    pub fn resolve<'a>(&self, snapshot: &'a Snapshot) -> Result<&'a Element> {
        let matches = self.find(snapshot);
        match matches.as_slice() {
            [] => Err(Error::not_found(format!("no element matches {self}"))),
            &[only] => Ok(only),
            _ => self.best_ranked(&matches).ok_or_else(|| self.ambiguous(&matches)),
        }
    }

    pub fn ocr_text(&self) -> Option<&str> {
        self.terms().find_map(|term| match term {
            Term::Ocr(text) => Some(text.as_str()),
            _ => None,
        })
    }

    pub fn nth(&self) -> Option<u32> {
        self.terms().find_map(Term::position)
    }

    pub fn plain_text(&self) -> Option<&str> {
        let mut element_terms = self.terms().filter(|term| term.is_element_term());
        match (element_terms.next(), element_terms.next()) {
            (Some(Term::Text(text) | Term::Ocr(text)), None) => Some(text.as_str()),
            _ => None,
        }
    }

    fn check_ocr(&self) -> Result<()> {
        let texts = self.terms().filter(|term| matches!(term, Term::Ocr(_))).count();
        let combinable =
            self.terms().all(|term| matches!(term, Term::Ocr(_) | Term::Window(_) | Term::Process(_) | Term::Nth(_)));
        if texts == 0 || (texts == 1 && combinable) {
            return Ok(());
        }
        Err(Error::invalid(OCR_ALONE))
    }

    fn terms(&self) -> impl Iterator<Item = &Term> {
        self.steps.iter().flat_map(|step| &step.terms)
    }

    fn final_texts(&self) -> Vec<&str> {
        self.steps
            .iter()
            .rev()
            .find(|step| step.filters_elements())
            .map(|step| step.texts().collect())
            .unwrap_or_default()
    }

    fn best_ranked<'a>(&self, matches: &[&'a Element]) -> Option<&'a Element> {
        let texts = self.final_texts();
        let ranked: Vec<(Rank, &'a Element)> =
            matches.iter().map(|&element| (rank(element, &texts), element)).collect();
        let best = ranked.iter().map(|(rank, _)| *rank).max()?;
        let mut winners = ranked.iter().filter(|(rank, _)| *rank == best);
        match (winners.next(), winners.next()) {
            (Some(&(_, element)), None) => Some(element),
            _ => None,
        }
    }

    fn ambiguous(&self, matches: &[&Element]) -> Error {
        let mut lines = vec![format!("{} elements match {self}; add nth:, a role:, or use a ref:", matches.len())];
        let listed = matches.iter().take(MAX_LISTED_CANDIDATES);
        lines.extend(listed.map(|element| format!("  {}", element_line(element, false))));
        if matches.len() > MAX_LISTED_CANDIDATES {
            lines.push("  …".to_string());
        }
        Error::new(ErrorCode::Ambiguous, lines.join("\n"))
    }
}

impl SelectorStep {
    fn has_element_terms(&self) -> bool {
        self.terms.iter().any(Term::is_element_term)
    }

    fn filters_elements(&self) -> bool {
        self.terms.iter().any(|term| !term.is_window_term())
    }

    fn matches(&self, element: &Element) -> bool {
        self.terms.iter().all(|term| term.matches_element(element))
    }

    fn nth(&self) -> Option<u32> {
        self.terms.iter().find_map(Term::position)
    }

    fn texts(&self) -> impl Iterator<Item = &str> {
        self.terms.iter().filter_map(|term| match term {
            Term::Text(text) => Some(text.as_str()),
            _ => None,
        })
    }

    fn select(&self, elements: &[Element], candidates: impl IntoIterator<Item = usize>) -> Vec<usize> {
        let matched = candidates
            .into_iter()
            .filter(|&index| elements.get(index).is_some_and(|element| self.matches(element)))
            .collect::<Vec<_>>();
        match self.nth() {
            Some(position) => nth_of(&preferring_visible(elements, matched), position).into_iter().collect(),
            None => matched,
        }
    }
}

fn preferring_visible(elements: &[Element], matched: Vec<usize>) -> Vec<usize> {
    let visible: Vec<usize> =
        matched.iter().copied().filter(|&index| elements.get(index).is_some_and(Element::is_visible)).collect();
    if visible.is_empty() { matched } else { visible }
}

fn nth_of(matched: &[usize], position: u32) -> Option<usize> {
    let index = usize::try_from(position).ok()?.checked_sub(1)?;
    matched.get(index).copied()
}

fn descendant_indices(snapshot: &Snapshot, parents: &[usize]) -> Vec<usize> {
    let mut indices = Vec::new();
    let mut covered_until = 0;
    for &parent in parents {
        if parent < covered_until {
            continue;
        }
        let start = parent + 1;
        let end = start + snapshot.descendants(parent).len();
        indices.extend(start..end);
        covered_until = end;
    }
    indices
}

pub fn is_selector_key(key: &str) -> bool {
    term_builder(&key.to_ascii_lowercase()).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        element::fixtures::{element, snapshot, window, with_value},
        geometry::Rect,
        role::Role,
    };

    fn parse(text: &str) -> Selector {
        text.parse().unwrap()
    }

    fn terms(text: &str) -> Vec<Term> {
        let selector = parse(text);
        assert_eq!(selector.steps.len(), 1, "{text}");
        selector.steps.into_iter().next().unwrap().terms
    }

    fn parse_error(text: &str) -> String {
        let error = text.parse::<Selector>().unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument, "{text}");
        error.message
    }

    fn canonical(text: &str) -> String {
        parse(text).to_string()
    }

    fn notepad() -> Snapshot {
        let mut editor = with_value(element(6, 1, Role::Edit, "Text editor"), "hello world");
        editor.automation_id = "15".to_string();
        editor.class_name = "RichEditD2DPT".to_string();
        editor.state.focused = true;
        snapshot(vec![
            element(1, 0, Role::Window, "Untitled - Notepad"),
            element(2, 1, Role::MenuBar, "Application"),
            element(3, 2, Role::MenuItem, "File"),
            element(4, 2, Role::MenuItem, "Edit"),
            element(5, 2, Role::MenuItem, "Format"),
            editor,
            element(7, 1, Role::StatusBar, "Status"),
            element(8, 2, Role::Text, "Ln 1, Col 1"),
            element(9, 1, Role::Button, "Save"),
            element(10, 1, Role::Button, "Save as"),
            element(11, 1, Role::Button, "Don't save"),
        ])
    }

    fn nested() -> Snapshot {
        snapshot(vec![
            element(1, 0, Role::Window, "Dialog"),
            element(2, 1, Role::Group, "Outer"),
            element(3, 2, Role::Button, "A"),
            element(4, 2, Role::Group, "Inner"),
            element(5, 3, Role::Button, "B"),
            element(6, 2, Role::Button, "C"),
            element(7, 1, Role::Group, "Second"),
            element(8, 2, Role::Button, "D"),
            element(9, 1, Role::Button, "E"),
        ])
    }

    fn found(selector: &str, snapshot: &Snapshot) -> Vec<u32> {
        parse(selector).find(snapshot).iter().map(|element| element.id.0).collect()
    }

    fn resolved(selector: &str, snapshot: &Snapshot) -> u32 {
        parse(selector).resolve(snapshot).unwrap().id.0
    }

    fn resolve_error(selector: &str, snapshot: &Snapshot) -> Error {
        parse(selector).resolve(snapshot).unwrap_err()
    }

    const NONE: [u32; 0] = [];

    #[test]
    fn parses_every_key() {
        let text = "role:button name:Save name~:sav id:1001 class:Edit text:hello window:Notepad process:notepad.exe nth:2 focused";
        assert_eq!(
            terms(text),
            [
                Term::Role(Role::Button),
                Term::Name("Save".to_string()),
                Term::NameContains("sav".to_string()),
                Term::Id("1001".to_string()),
                Term::Class("Edit".to_string()),
                Term::Text("hello".to_string()),
                Term::Window("Notepad".to_string()),
                Term::Process("notepad.exe".to_string()),
                Term::Nth(2),
                Term::Focused,
            ]
        );
        assert_eq!(canonical(text), text);
    }

    #[test]
    fn keys_are_case_insensitive() {
        assert_eq!(terms("ROLE:Button Name~:x"), [Term::Role(Role::Button), Term::NameContains("x".to_string())]);
    }

    #[test]
    fn roles_parse_with_role_from_str() {
        assert_eq!(terms("role:hyperlink"), [Term::Role(Role::Link)]);
        assert_eq!(terms("role:\"menu item\""), [Term::Role(Role::MenuItem)]);
        assert!(parse_error("role:gizmo").starts_with("unknown role 'gizmo'; roles are button, "));
        assert_eq!(terms("role:option"), [Term::Role(Role::ListItem)]);
    }

    #[test]
    fn bare_words_join_into_one_text_term() {
        assert_eq!(terms("Save as"), [Term::Text("Save as".to_string())]);
        assert_eq!(terms("  Save    as  "), [Term::Text("Save as".to_string())]);
        assert_eq!(terms("\"Save as\""), [Term::Text("Save as".to_string())]);
        assert_eq!(terms("\"Save\" as"), [Term::Text("Save as".to_string())]);
    }

    #[test]
    fn keyed_terms_split_bare_text_runs() {
        assert_eq!(
            terms("Save role:button as"),
            [Term::Text("Save".to_string()), Term::Role(Role::Button), Term::Text("as".to_string())]
        );
    }

    #[test]
    fn focused_keyword_is_bare_and_case_insensitive() {
        assert_eq!(terms("focused"), [Term::Focused]);
        assert_eq!(terms("FOCUSED role:edit"), [Term::Focused, Term::Role(Role::Edit)]);
        assert_eq!(terms("\"focused\""), [Term::Text("focused".to_string())]);
        assert_eq!(terms("text:focused"), [Term::Text("focused".to_string())]);
        assert_eq!(
            terms("Save focused as"),
            [Term::Text("Save".to_string()), Term::Focused, Term::Text("as".to_string())]
        );
    }

    #[test]
    fn quoted_values_unescape_quotes_and_backslashes() {
        assert_eq!(terms(r#"name:"say \"hi\" \\ now""#), [Term::Name(r#"say "hi" \ now"#.to_string())]);
        assert_eq!(terms(r#"name:"C:\temp""#), [Term::Name(r"C:\temp".to_string())]);
        assert_eq!(terms(r"name:C:\temp"), [Term::Name(r"C:\temp".to_string())]);
    }

    #[test]
    fn quoted_values_may_hold_spaces_and_chain_marks() {
        let selector = parse("name:\"a >> b\" >> role:button");
        assert_eq!(selector.steps.len(), 2);
        assert_eq!(selector.steps[0].terms, [Term::Name("a >> b".to_string())]);
        assert_eq!(selector.steps[1].terms, [Term::Role(Role::Button)]);
    }

    #[test]
    fn chains_need_no_spaces() {
        let selector = parse("role:menubar>>name:File");
        assert_eq!(selector.steps.len(), 2);
        assert_eq!(selector.steps[1].terms, [Term::Name("File".to_string())]);
        assert_eq!(parse("\"Save\">>role:button").steps.len(), 2);
    }

    #[test]
    fn values_are_trimmed_except_ids() {
        assert_eq!(terms("name:\"  Save  \""), [Term::Name("Save".to_string())]);
        assert_eq!(terms("id:\" x \""), [Term::Id(" x ".to_string())]);
    }

    #[test]
    fn colons_inside_values() {
        assert_eq!(terms("name:a:b"), [Term::Name("a:b".to_string())]);
        assert_eq!(terms("12:30"), [Term::Text("12:30".to_string())]);
        assert_eq!(terms(":x"), [Term::Text(":x".to_string())]);
        assert_eq!(terms("\"http://x\""), [Term::Text("http://x".to_string())]);
    }

    #[test]
    fn unknown_keys_are_rejected() {
        let expected = "'foo:' is not a selector key; put text containing ':' in quotes, e.g. \"File name:\". Keys are role, name, name~, id, class, text, ocr, window, process and nth, plus the bare word focused";
        assert_eq!(parse_error("foo:bar"), expected);
        assert_eq!(parse_error("foo:"), expected);
        assert!(parse_error("Role~:x").starts_with("'role~:' is not a selector key;"));
        assert!(parse_error("focused:yes").starts_with("'focused:' is not a selector key;"));
        assert!(parse_error("http://x").starts_with("'http:' is not a selector key;"));
    }

    #[test]
    fn plain_text_with_a_colon_explains_quoting() {
        assert!(
            parse_error("Note: save first")
                .starts_with("'note:' is not a selector key; put text containing ':' in quotes, e.g. \"File name:\"")
        );
        assert_eq!(
            parse_error("File name:"),
            "empty value for 'name:' in selector; to match text, put text containing ':' in quotes, e.g. \"File name:\""
        );
        assert_eq!(terms("\"Note: save first\""), [Term::Text("Note: save first".to_string())]);
        assert_eq!(terms("\"File name:\""), [Term::Text("File name:".to_string())]);
    }

    #[test]
    fn empty_values_are_rejected() {
        let hint = "; to match text, put text containing ':' in quotes, e.g. \"File name:\"";
        assert_eq!(parse_error("name:"), format!("empty value for 'name:' in selector{hint}"));
        assert_eq!(parse_error("name: Save"), format!("empty value for 'name:' in selector{hint}"));
        assert_eq!(parse_error("name:\"\""), "empty value for 'name:' in selector");
        assert_eq!(parse_error("name:\"   \""), "empty value for 'name:' in selector");
        assert_eq!(parse_error("id:\"  \""), "empty value for 'id:' in selector");
        assert_eq!(parse_error("role:>>x"), format!("empty value for 'role:' in selector{hint}"));
        assert_eq!(parse_error("\"\""), "empty quoted value in selector");
        assert_eq!(parse_error("role:button \"  \""), "empty quoted value in selector");
    }

    #[test]
    fn unterminated_quotes_are_rejected() {
        assert_eq!(parse_error("name:\"Save"), "unterminated quote in selector at '\"Save'");
        assert_eq!(parse_error("\"abc"), "unterminated quote in selector at '\"abc'");
        assert_eq!(parse_error(r#"name:"a\""#), r#"unterminated quote in selector at '"a\"'"#);
        assert_eq!(parse_error(r#"name:"a\"#), r#"unterminated quote in selector at '"a\'"#);
    }

    #[test]
    fn text_glued_to_a_closing_quote_is_rejected() {
        assert_eq!(parse_error("name:\"a\"b c"), "unexpected 'b' right after a closing quote in selector");
    }

    #[test]
    fn ocr_terms_parse_and_print_back() {
        assert_eq!(terms("ocr:\"Export video\""), [Term::Ocr("Export video".to_string())]);
        assert_eq!(canonical("ocr:\"Export video\""), "ocr:\"Export video\"");
        assert_eq!(canonical("OCR:Export"), "ocr:Export");
        let scoped = parse("window:CapCut ocr:Export nth:2");
        assert_eq!(scoped.ocr_text(), Some("Export"));
        assert_eq!(scoped.nth(), Some(2));
        assert!(scoped.has_window_terms());
        let chained = parse("process:capcut >> ocr:\"Media\"");
        assert_eq!(chained.ocr_text(), Some("Media"));
        assert_eq!(chained.nth(), None);
    }

    #[test]
    fn ocr_combines_only_with_window_process_and_nth() {
        let expected = "ocr: takes one text and combines only with window:, process: and nth:";
        assert_eq!(parse_error("ocr:Export role:button"), expected);
        assert_eq!(parse_error("ocr:Export Save"), expected);
        assert_eq!(parse_error("ocr:a ocr:b"), expected);
        assert_eq!(parse_error("role:pane >> ocr:Export"), expected);
        assert_eq!(parse_error("focused ocr:Export"), expected);
        assert_eq!(parse_error("ocr:\" \""), "empty value for 'ocr:' in selector");
    }

    #[test]
    fn ocr_selectors_never_match_elements() {
        let snapshot = notepad();
        assert_eq!(found("ocr:File", &snapshot), NONE);
        assert_eq!(resolve_error("ocr:File", &snapshot).code, ErrorCode::NotFound);
        assert_eq!(parse("name:File").ocr_text(), None);
    }

    #[test]
    fn plain_text_is_a_lone_text_or_ocr_term() {
        assert_eq!(parse("Export").plain_text(), Some("Export"));
        assert_eq!(parse("window:CapCut text:\"Export video\" nth:1").plain_text(), Some("Export video"));
        assert_eq!(parse("ocr:Media").plain_text(), Some("Media"));
        assert_eq!(parse("role:button Export").plain_text(), None);
        assert_eq!(parse("Save role:button as").plain_text(), None);
        assert_eq!(parse("window:CapCut").plain_text(), None);
        assert_eq!(parse("name:Export").plain_text(), None);
    }

    #[test]
    fn nth_must_be_a_positive_whole_number() {
        for value in ["0", "x", "-1", "1.5", "99999999999"] {
            assert_eq!(parse_error(&format!("nth:{value}")), format!("nth needs a whole number from 1, got '{value}'"));
        }
        assert_eq!(terms("nth:1"), [Term::Nth(1)]);
        assert_eq!(terms("nth:\"3\""), [Term::Nth(3)]);
    }

    #[test]
    fn empty_steps_are_rejected() {
        assert_eq!(parse_error("a >> >> b"), "empty step in selector 'a >> >> b'");
        assert_eq!(parse_error(">> a"), "empty step in selector '>> a'");
        assert_eq!(parse_error("a >>"), "empty step in selector 'a >>'");
        assert_eq!(parse_error(">>"), "empty step in selector '>>'");
        assert_eq!(parse_error("a>>>>b"), "empty step in selector 'a>>>>b'");
    }

    #[test]
    fn empty_selectors_are_rejected() {
        assert_eq!(parse_error(""), "empty selector");
        assert_eq!(parse_error("  \t "), "empty selector");
    }

    #[test]
    fn display_is_canonical() {
        assert_eq!(canonical("ROLE:Button   NAME:\"Save\""), "role:button name:Save");
        assert_eq!(canonical("Save as"), "text:\"Save as\"");
        assert_eq!(canonical("role:hyperlink"), "role:link");
        assert_eq!(canonical("Focused"), "focused");
        assert_eq!(
            canonical("window:\"Untitled - Notepad\">>role:menuitem   name:File"),
            "window:\"Untitled - Notepad\" >> role:menuitem name:File"
        );
        assert_eq!(canonical(r#"name:"say \"hi\"""#), r#"name:"say \"hi\"""#);
        assert_eq!(canonical(r"name:C:\temp"), r#"name:"C:\\temp""#);
        assert_eq!(canonical("name:a>b"), "name:\"a>b\"");
        assert_eq!(canonical("id:\" x \""), "id:\" x \"");
        assert_eq!(canonical("nth:007"), "nth:7");
    }

    #[test]
    fn display_quotes_empty_values() {
        let selector = Selector { steps: vec![SelectorStep { terms: vec![Term::Name(String::new())] }] };
        assert_eq!(selector.to_string(), "name:\"\"");
    }

    #[test]
    fn display_round_trips() {
        let cases = [
            "role:button name:\"Save\"",
            "Save as",
            "\"a >> b\" >> role:button",
            r#"name:"tab\there""#,
            r#"name:"quote \" and backslash \\""#,
            "window:Notepad process:notepad >> role:menubar >> name~:fil nth:2",
            "focused role:edit",
            "\"focused\"",
            "text:\"focused\"",
            "id:\"  spaced id \"",
            r#"class:"a\\b""#,
            "name:ü-umlaut",
            "name:\"日本語 テキスト\"",
            "role:\"menu item\"",
            "12:30",
            "name:a:b",
            "name:\"line\nbreak\"",
            "Save focused as",
        ];
        for text in cases {
            let selector = parse(text);
            let shown = selector.to_string();
            assert_eq!(parse(&shown), selector, "{text} -> {shown}");
            assert_eq!(parse(&shown).to_string(), shown, "{text}");
        }
    }

    #[test]
    fn window_and_element_term_detection() {
        assert!(parse("window:Notepad").has_window_terms());
        assert!(!parse("window:Notepad").has_element_terms());
        assert!(!parse("role:button").has_window_terms());
        assert!(parse("role:button").has_element_terms());
        assert!(parse("process:notepad >> name:File").has_window_terms());
        assert!(parse("process:notepad >> name:File").has_element_terms());
        assert!(!parse("nth:2").has_element_terms());
        assert!(!parse("window:Notepad nth:2").has_element_terms());
        assert!(parse("window:Notepad nth:2").has_window_terms());
        assert!(parse("name:Outer >> nth:2").has_element_terms());
        assert!(parse("focused").has_element_terms());
    }

    #[test]
    fn window_and_nth_alone_find_no_elements() {
        let snap = snapshot(vec![element(1, 0, Role::Window, "Untitled - Notepad"), element(2, 1, Role::Button, "OK")]);
        assert_eq!(found("window:Notepad nth:2", &snap), NONE);
        assert_eq!(found("nth:1", &snap), NONE);
        assert_eq!(found("window:Notepad nth:1 >> role:button", &snap), [2]);
    }

    #[test]
    fn window_titles_match_by_contains() {
        let notepad = window("Untitled - Notepad");
        assert!(parse("window:notepad").matches_window(&notepad));
        assert!(parse("window:\"untitled -\"").matches_window(&notepad));
        assert!(parse("window:\"  NOTEPAD  \"").matches_window(&notepad));
        assert!(!parse("window:Chrome").matches_window(&notepad));
    }

    #[test]
    fn processes_match_with_or_without_exe() {
        let notepad = window("Untitled - Notepad");
        assert!(parse("process:notepad").matches_window(&notepad));
        assert!(parse("process:NOTEPAD.EXE").matches_window(&notepad));
        assert!(!parse("process:note").matches_window(&notepad));
        assert!(!parse("process:chrome").matches_window(&notepad));
        let bare = WindowInfo { process: "Notepad".to_string(), ..window("x") };
        assert!(parse("process:notepad.exe").matches_window(&bare));
    }

    #[test]
    fn every_window_term_in_every_step_must_match() {
        let notepad = window("Untitled - Notepad");
        assert!(parse("window:Notepad >> process:notepad >> role:button").matches_window(&notepad));
        assert!(!parse("window:Notepad >> process:chrome").matches_window(&notepad));
        assert!(parse("role:button").matches_window(&notepad));
    }

    #[test]
    fn find_by_each_element_term() {
        let snap = notepad();
        assert_eq!(found("role:menuitem", &snap), [3, 4, 5]);
        assert_eq!(found("name:file", &snap), [3]);
        assert_eq!(found("name:Fil", &snap), NONE);
        assert_eq!(found("name~:ORM", &snap), [5]);
        assert_eq!(found("id:15", &snap), [6]);
        assert_eq!(found("class:richeditd2dpt", &snap), [6]);
        assert_eq!(found("text:editor", &snap), [6]);
        assert_eq!(found("text:WORLD", &snap), [6]);
        assert_eq!(found("text:save", &snap), [9, 10, 11]);
        assert_eq!(found("Save", &snap), [9, 10, 11]);
        assert_eq!(found("focused", &snap), [6]);
        assert_eq!(found("role:button focused", &snap), NONE);
        assert_eq!(found("role:button name~:save", &snap), [9, 10, 11]);
    }

    #[test]
    fn ids_are_case_sensitive_and_untrimmed() {
        let mut button = element(1, 0, Role::Button, "Save");
        button.automation_id = "SaveBtn".to_string();
        let snap = snapshot(vec![button]);
        assert_eq!(found("id:SaveBtn", &snap), [1]);
        assert_eq!(found("id:savebtn", &snap), NONE);
        assert_eq!(found("id:\" SaveBtn\"", &snap), NONE);
    }

    #[test]
    fn names_shown_by_observe_paste_back_into_selectors() {
        let names =
            ["line one\nline two", "a\tb", "  spaced   out  ", "crlf\r\nend", "bell\u{7}name", "quote \" and \\"];
        let snap = snapshot(
            names.iter().enumerate().map(|(index, name)| element(index as u32 + 1, 0, Role::Text, name)).collect(),
        );
        for (index, element) in snap.elements.iter().enumerate() {
            let line = element_line(element, false);
            let shown =
                line.split_once(' ').and_then(|(_, rest)| rest.split_once(' ')).map(|(_, label)| label).unwrap();
            assert_eq!(found(&format!("name:{shown}"), &snap), [index as u32 + 1], "{line}");
        }
    }

    #[test]
    fn whitespace_runs_and_controls_fold_to_single_spaces() {
        let snap = snapshot(vec![element(1, 0, Role::Text, "line one\nline two"), element(2, 0, Role::Text, "a\tb")]);
        assert_eq!(found("name:\"line one line two\"", &snap), [1]);
        assert_eq!(found("name:\"LINE   ONE line two\"", &snap), [1]);
        assert_eq!(found("name:\"a b\"", &snap), [2]);
        assert_eq!(found("text:\"one line\"", &snap), [1]);
    }

    #[test]
    fn element_strings_are_trimmed_before_comparing() {
        let mut padded = element(1, 0, Role::Button, "  Padded  ");
        padded.class_name = " Button ".to_string();
        let snap = snapshot(vec![padded]);
        assert_eq!(found("name:padded", &snap), [1]);
        assert_eq!(found("class:button", &snap), [1]);
    }

    #[test]
    fn chained_steps_match_descendants_only() {
        let snap = notepad();
        assert_eq!(found("role:menubar >> name:Edit", &snap), [4]);
        assert_eq!(found("role:menubar >> role:button", &snap), NONE);
        assert_eq!(found("role:statusbar >> role:text", &snap), [8]);
        assert_eq!(found("role:window >> role:button", &snap), [9, 10, 11]);
    }

    #[test]
    fn nested_parents_give_no_duplicates_in_document_order() {
        let snap = nested();
        assert_eq!(found("role:group >> role:button", &snap), [3, 5, 6, 8]);
        assert_eq!(found("role:group >> role:group", &snap), [4]);
        assert_eq!(found("name:Outer >> role:group >> role:button", &snap), [5]);
        assert_eq!(found("role:window >> role:group >> role:button", &snap), [3, 5, 6, 8]);
    }

    #[test]
    fn nth_picks_within_its_step() {
        let snap = nested();
        assert_eq!(found("role:button nth:2", &snap), [5]);
        assert_eq!(found("role:button nth:5", &snap), [9]);
        assert_eq!(found("role:button nth:6", &snap), NONE);
        assert_eq!(found("role:group nth:2 >> role:button", &snap), [5]);
        assert_eq!(found("role:group nth:3 >> role:button", &snap), [8]);
        assert_eq!(found("role:group >> role:button nth:3", &snap), [6]);
        assert_eq!(found("name:Outer >> nth:2", &snap), [4]);
        assert_eq!(found("role:menubar >> role:menuitem nth:2", &notepad()), [4]);
    }

    #[test]
    fn nth_counts_visible_matches_first() {
        let mut hidden = element(3, 1, Role::Button, "B");
        hidden.state.offscreen = true;
        let mut zero = element(5, 1, Role::Button, "Z");
        zero.rect = Rect::new(0, 0, 0, 0);
        let snap = snapshot(vec![
            element(1, 0, Role::Window, "App"),
            element(2, 1, Role::Button, "A"),
            hidden,
            zero,
            element(4, 1, Role::Button, "C"),
        ]);
        assert_eq!(found("role:button nth:2", &snap), [4]);
        assert_eq!(found("role:button nth:3", &snap), NONE);
        assert_eq!(resolved("role:button nth:2", &snap), 4);
        assert_eq!(found("role:button", &snap), [2, 3, 5, 4]);
    }

    #[test]
    fn nth_falls_back_to_hidden_matches_when_none_are_visible() {
        let hidden = |id, name| {
            let mut button = element(id, 0, Role::Button, name);
            button.state.offscreen = true;
            button
        };
        let snap = snapshot(vec![hidden(1, "A"), hidden(2, "B"), element(3, 0, Role::Text, "Label")]);
        assert_eq!(found("role:button nth:2", &snap), [2]);
        assert_eq!(found("role:button nth:3", &snap), NONE);
    }

    #[test]
    fn window_terms_filter_the_whole_snapshot() {
        let snap = notepad();
        assert_eq!(found("window:Notepad >> role:menubar >> name:File", &snap), [3]);
        assert_eq!(found("window:Chrome >> role:menubar", &snap), NONE);
        assert_eq!(found("process:chrome role:button", &snap), NONE);
        assert_eq!(found("process:NOTEPAD.EXE role:button", &snap), [9, 10, 11]);
        assert_eq!(found("role:button >> window:Notepad", &snap), [9, 10, 11]);
        assert_eq!(found("role:button >> window:Chrome", &snap), NONE);
    }

    #[test]
    fn window_only_selectors_find_no_elements() {
        assert!(parse("window:Notepad").find(&notepad()).is_empty());
        assert!(parse("window:Notepad >> process:notepad").find(&notepad()).is_empty());
    }

    #[test]
    fn find_on_an_empty_snapshot() {
        assert!(parse("role:button").find(&snapshot(Vec::new())).is_empty());
        assert!(parse("a >> b").find(&snapshot(Vec::new())).is_empty());
    }

    #[test]
    fn resolve_reports_not_found_with_the_canonical_selector() {
        let error = resolve_error("ROLE:Button  name:Nope", &notepad());
        assert_eq!(error.code, ErrorCode::NotFound);
        assert_eq!(error.message, "no element matches role:button name:Nope");
        let error = resolve_error("window:Chrome >> Save", &notepad());
        assert_eq!(error.message, "no element matches window:Chrome >> text:Save");
    }

    #[test]
    fn resolve_returns_a_single_match() {
        assert_eq!(resolved("name:File", &notepad()), 3);
    }

    #[test]
    fn exact_name_beats_prefix_beats_contains() {
        assert_eq!(resolved("text:save", &notepad()), 9);
        let snap = snapshot(vec![element(1, 0, Role::Button, "Don't save"), element(2, 0, Role::Button, "Save as")]);
        assert_eq!(resolved("text:save", &snap), 2);
    }

    #[test]
    fn value_matches_rank_like_contains() {
        let by_value = with_value(element(3, 0, Role::Edit, "Search"), "save");
        let snap = snapshot(vec![by_value, element(4, 0, Role::Button, "Autosave")]);
        assert_eq!(found("text:save", &snap), [3, 4]);
        assert_eq!(resolve_error("text:save", &snap).code, ErrorCode::Ambiguous);
    }

    #[test]
    fn visible_beats_hidden() {
        let mut offscreen = element(1, 0, Role::Button, "OK");
        offscreen.state.offscreen = true;
        let snap = snapshot(vec![offscreen, element(2, 0, Role::Button, "OK")]);
        assert_eq!(resolved("name:OK", &snap), 2);
        let mut empty = element(3, 0, Role::Button, "OK");
        empty.rect = Rect::new(10, 10, 0, 20);
        let snap = snapshot(vec![empty, element(4, 0, Role::Button, "OK")]);
        assert_eq!(resolved("name:OK", &snap), 4);
    }

    #[test]
    fn enabled_beats_disabled() {
        let mut disabled = element(1, 0, Role::Button, "OK");
        disabled.state.enabled = false;
        let snap = snapshot(vec![disabled, element(2, 0, Role::Button, "OK")]);
        assert_eq!(resolved("name:OK", &snap), 2);
    }

    #[test]
    fn actionable_beats_inert() {
        let snap = snapshot(vec![element(1, 0, Role::Text, "Total"), element(2, 0, Role::Button, "Total")]);
        assert_eq!(resolved("name:Total", &snap), 2);
        let mut invokable = element(3, 0, Role::Text, "Total");
        invokable.patterns.invoke = true;
        let snap = snapshot(vec![element(4, 0, Role::Text, "Total"), invokable]);
        assert_eq!(resolved("name:Total", &snap), 3);
    }

    #[test]
    fn text_rank_outweighs_visibility() {
        let mut exact_hidden = element(1, 0, Role::Button, "Save");
        exact_hidden.state.offscreen = true;
        let snap = snapshot(vec![exact_hidden, element(2, 0, Role::Button, "Save as")]);
        assert_eq!(resolved("text:save", &snap), 1);
    }

    #[test]
    fn visibility_outweighs_enabled() {
        let mut visible_disabled = element(3, 0, Role::Button, "OK");
        visible_disabled.state.enabled = false;
        let mut hidden_enabled = element(4, 0, Role::Button, "OK");
        hidden_enabled.state.offscreen = true;
        assert_eq!(resolved("name:OK", &snapshot(vec![hidden_enabled, visible_disabled])), 3);
    }

    #[test]
    fn enabled_outweighs_actionable() {
        let mut disabled_button = element(5, 0, Role::Button, "Total");
        disabled_button.state.enabled = false;
        let snap = snapshot(vec![disabled_button, element(6, 0, Role::Text, "Total")]);
        assert_eq!(resolved("name:Total", &snap), 6);
    }

    #[test]
    fn ties_are_ambiguous_and_list_candidates() {
        let error = resolve_error("text:sav", &notepad());
        assert_eq!(error.code, ErrorCode::Ambiguous);
        assert_eq!(
            error.message,
            "3 elements match text:sav; add nth:, a role:, or use a ref:\n  e9 button \"Save\"\n  e10 button \"Save as\"\n  e11 button \"Don't save\""
        );
    }

    #[test]
    fn ambiguity_lists_at_most_five_candidates() {
        let buttons = (1..=7).map(|id| element(id, 0, Role::Button, &format!("Item {id}"))).collect();
        let error = resolve_error("role:button", &snapshot(buttons));
        let lines: Vec<&str> = error.message.lines().collect();
        assert_eq!(lines.len(), 7);
        assert_eq!(lines[0], "7 elements match role:button; add nth:, a role:, or use a ref:");
        assert_eq!(lines[1], "  e1 button \"Item 1\"");
        assert_eq!(lines[5], "  e5 button \"Item 5\"");
        assert_eq!(lines[6], "  …");
    }

    #[test]
    fn exactly_five_candidates_have_no_ellipsis() {
        let buttons = (1..=5).map(|id| element(id, 0, Role::Button, "Same")).collect();
        let error = resolve_error("name:Same", &snapshot(buttons));
        assert_eq!(error.message.lines().count(), 6);
        assert!(!error.message.contains('…'));
    }

    #[test]
    fn only_the_final_steps_text_terms_rank() {
        let error = resolve_error("text:Outer >> role:button", &nested());
        assert_eq!(error.code, ErrorCode::Ambiguous);
        assert!(error.message.starts_with("3 elements match text:Outer >> role:button;"));
    }

    #[test]
    fn nth_resolves_ambiguity() {
        assert_eq!(resolved("text:sav nth:2", &notepad()), 10);
    }
}
