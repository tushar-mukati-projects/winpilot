use std::time::Instant;

use deckhand_core::{Element, ElementRef, Result, Role, Snapshot, WindowInfo, WindowRef};
use deckhand_desktop::VisibleWindow;

use super::target::Prepared;
use crate::{engine::Engine, resolve::Reach, wait::unless_gone, window_classes::is_popup_class};

pub const LISTED_NAMES: usize = 10;

#[derive(Debug, Clone)]
pub struct Combo {
    pub window: WindowInfo,
    pub id: ElementRef,
    pub opened_at: Instant,
}

#[derive(Debug, Clone)]
pub struct Found {
    pub element: Element,
    pub window: WindowInfo,
}

#[derive(Debug, Clone, Default)]
pub struct Search {
    pub found: Option<Found>,
    pub names: Vec<String>,
    pub popup: Option<WindowRef>,
}

impl Found {
    pub fn prepared(&self) -> Prepared {
        Prepared { window: self.window.clone(), reach: Reach::Element(self.element.clone()) }
    }
}

impl Search {
    fn joined(mut self, later: Self) -> Self {
        self.names.extend(later.names);
        Self { found: later.found, names: dedup(self.names), popup: later.popup.or(self.popup) }
    }
}

pub fn search(engine: &Engine, combo: &Combo, option: &str) -> Result<Search> {
    let mine = in_subtree(engine, &combo.window, combo.id, option)?;
    if mine.found.is_some() {
        return Ok(mine);
    }
    let popups = mine.joined(in_popups(engine, combo, option)?);
    if popups.found.is_some() {
        return Ok(popups);
    }
    Ok(popups.joined(in_opened_windows(engine, combo, option)?))
}

pub fn in_subtree(engine: &Engine, window: &WindowInfo, id: ElementRef, option: &str) -> Result<Search> {
    let own = engine.snapshot(&deckhand_desktop::window(window.hwnd)?)?;
    let subtree = subtree_of(&own, id);
    Ok(scope_search(subtree, &own.window, option, false))
}

fn in_popups(engine: &Engine, combo: &Combo, option: &str) -> Result<Search> {
    let mut search = Search::default();
    for popup in same_process_popups(combo)? {
        let Some(snapshot) = snapshot_of(engine, popup.hwnd)? else { continue };
        let found =
            Search { popup: Some(popup.hwnd), ..scope_search(&snapshot.elements, &snapshot.window, option, false) };
        search = search.joined(found);
        if search.found.is_some() {
            break;
        }
    }
    Ok(search)
}

fn in_opened_windows(engine: &Engine, combo: &Combo, option: &str) -> Result<Search> {
    let mut search = Search::default();
    for hwnd in engine.changes.windows_opened_since(combo.opened_at) {
        let Some(window) = unless_gone(deckhand_desktop::window(hwnd))? else { continue };
        if !opened_by_combo_app(&window, combo) || engine.guard.denies(&window) {
            continue;
        }
        let Some(snapshot) = unless_gone(engine.snapshot(&window))? else { continue };
        search = search.joined(scope_search(&snapshot.elements, &snapshot.window, option, true));
        if search.found.is_some() {
            break;
        }
    }
    Ok(search)
}

fn same_process_popups(combo: &Combo) -> Result<Vec<VisibleWindow>> {
    let windows = deckhand_desktop::all_visible_windows()?;
    Ok(windows.into_iter().filter(|window| is_combo_popup(window, combo)).collect())
}

pub fn is_combo_popup(window: &VisibleWindow, combo: &Combo) -> bool {
    window.pid == combo.window.pid && window.hwnd != combo.window.hwnd && is_popup_class(&window.class_name)
}

pub fn opened_by_combo_app(window: &WindowInfo, combo: &Combo) -> bool {
    window.pid == combo.window.pid && window.hwnd != combo.window.hwnd && !is_popup_class(&window.class_name)
}

fn snapshot_of(engine: &Engine, hwnd: WindowRef) -> Result<Option<Snapshot>> {
    let Some(window) = unless_gone(deckhand_desktop::window(hwnd))? else { return Ok(None) };
    if engine.guard.denies(&window) {
        return Ok(None);
    }
    unless_gone(engine.snapshot(&window))
}

fn scope_search(elements: &[Element], window: &WindowInfo, option: &str, items_only: bool) -> Search {
    let names = item_names(elements);
    let found = find_option(elements, option, items_only)
        .map(|element| Found { element: element.clone(), window: window.clone() });
    Search { found, names, popup: None }
}

pub fn subtree_of(snapshot: &Snapshot, id: ElementRef) -> &[Element] {
    snapshot.index_of(id).map_or(&[], |index| snapshot.descendants(index))
}

pub fn find_option<'a>(elements: &'a [Element], option: &str, items_only: bool) -> Option<&'a Element> {
    let wanted = fold(option);
    let mut matches = elements.iter().filter(|element| element.is_visible() && fold(&element.name) == wanted);
    let first = matches.next()?;
    if is_item(first) {
        return Some(first);
    }
    let item = matches.find(|element| is_item(element));
    if items_only { item } else { item.or(Some(first)) }
}

pub fn is_item(element: &Element) -> bool {
    matches!(element.role, Role::ListItem | Role::MenuItem | Role::TreeItem | Role::DataItem)
        || element.patterns.selection_item
}

pub fn item_names(elements: &[Element]) -> Vec<String> {
    let names = elements
        .iter()
        .filter(|element| element.is_visible() && is_item(element))
        .map(|element| element.name.trim().to_string())
        .filter(|name| !name.is_empty());
    dedup(names.collect())
}

fn dedup(names: Vec<String>) -> Vec<String> {
    let mut unique: Vec<String> = Vec::with_capacity(names.len().min(LISTED_NAMES + 1));
    for name in names {
        if unique.len() > LISTED_NAMES {
            break;
        }
        if !unique.contains(&name) {
            unique.push(name);
        }
    }
    unique
}

fn fold(text: &str) -> String {
    text.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use deckhand_core::{ElementState, Patterns, Rect};

    use super::*;
    use crate::test_fixture::window;

    fn element(id: u32, role: Role, name: &str, offscreen: bool) -> Element {
        Element {
            id: ElementRef(id),
            depth: 1,
            role,
            name: name.to_string(),
            value: None,
            automation_id: String::new(),
            class_name: String::new(),
            rect: Rect::new(0, 0, 10, 10),
            state: ElementState { enabled: true, offscreen, ..ElementState::default() },
            patterns: Patterns::default(),
        }
    }

    fn combo() -> Combo {
        let window = WindowInfo { pid: 7, ..window(1, "app.exe", "App", Rect::new(0, 0, 100, 100)) };
        Combo { window, id: ElementRef(3), opened_at: Instant::now() }
    }

    fn visible(hwnd: i64, pid: u32, class_name: &str) -> VisibleWindow {
        VisibleWindow {
            hwnd: WindowRef(hwnd),
            pid,
            process: "app.exe".to_string(),
            title: String::new(),
            class_name: class_name.to_string(),
            rect: Rect::new(0, 0, 10, 10),
        }
    }

    #[test]
    fn options_match_by_exact_name_ignoring_case() {
        let items = [element(1, Role::ListItem, "Green", false), element(2, Role::ListItem, "Greenish", false)];
        assert_eq!(find_option(&items, " green ", false).map(|found| found.id), Some(ElementRef(1)));
        assert_eq!(find_option(&items, "gree", false), None);
    }

    #[test]
    fn hidden_matches_are_skipped() {
        let items = [element(1, Role::ListItem, "Blue", true), element(2, Role::ListItem, "Blue", false)];
        assert_eq!(find_option(&items, "Blue", false).map(|found| found.id), Some(ElementRef(2)));
    }

    #[test]
    fn items_win_over_other_elements_with_the_same_name() {
        let mixed = [element(1, Role::Text, "Blue", false), element(2, Role::MenuItem, "Blue", false)];
        assert_eq!(find_option(&mixed, "Blue", false).map(|found| found.id), Some(ElementRef(2)));
    }

    #[test]
    fn a_lone_non_item_counts_only_where_allowed() {
        let lone = [element(1, Role::Button, "Blue", false)];
        assert_eq!(find_option(&lone, "Blue", false).map(|found| found.id), Some(ElementRef(1)));
        assert_eq!(find_option(&lone, "Blue", true), None);
    }

    #[test]
    fn item_names_list_visible_items_once() {
        let elements = [
            element(1, Role::ListItem, "Red", false),
            element(2, Role::ListItem, "Red", false),
            element(3, Role::ListItem, "Hidden", true),
            element(4, Role::Button, "OK", false),
            element(5, Role::TreeItem, " ", false),
            element(6, Role::MenuItem, "Green", false),
        ];
        assert_eq!(item_names(&elements), ["Red", "Green"]);
    }

    #[test]
    fn selectable_elements_count_as_items() {
        let mut custom = element(1, Role::Custom, "Option", false);
        assert!(!is_item(&custom));
        custom.patterns.selection_item = true;
        assert!(is_item(&custom));
    }

    #[test]
    fn name_lists_are_capped() {
        let many: Vec<String> = (0..30).map(|index| format!("Item {index}")).collect();
        assert_eq!(dedup(many).len(), LISTED_NAMES + 1);
    }

    #[test]
    fn only_the_combo_apps_own_popups_are_searched() {
        let combo = combo();
        assert!(is_combo_popup(&visible(5, 7, "ComboLBox"), &combo));
        assert!(is_combo_popup(&visible(6, 7, "#32768"), &combo));
        assert!(!is_combo_popup(&visible(7, 8, "#32768"), &combo));
        assert!(!is_combo_popup(&visible(8, 7, "Chrome_WidgetWin_1"), &combo));
        assert!(!is_combo_popup(&visible(1, 7, "ComboLBox"), &combo));
    }

    #[test]
    fn opened_windows_must_come_from_the_combo_app() {
        let combo = combo();
        let same_app = WindowInfo { pid: 7, ..window(9, "app.exe", "Picker", Rect::new(0, 0, 10, 10)) };
        let other_app = WindowInfo { pid: 8, ..same_app.clone() };
        let menu = WindowInfo { class_name: "#32768".to_string(), ..same_app.clone() };
        assert!(opened_by_combo_app(&same_app, &combo));
        assert!(!opened_by_combo_app(&other_app, &combo));
        assert!(!opened_by_combo_app(&menu, &combo));
        assert!(!opened_by_combo_app(&combo.window, &combo));
    }

    #[test]
    fn later_scopes_add_names_and_keep_the_popup() {
        let first = Search { found: None, names: vec!["Red".to_string()], popup: Some(WindowRef(5)) };
        let second = Search { found: None, names: vec!["Red".to_string(), "Blue".to_string()], popup: None };
        let joined = first.joined(second);
        assert_eq!(joined.names, ["Red", "Blue"]);
        assert_eq!(joined.popup, Some(WindowRef(5)));
    }
}
