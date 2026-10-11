use std::time::{Duration, Instant};

use deckhand_core::{Element, ElementRef, Error, Result, Role, WindowRef, keys::NamedKey};

use super::{
    chords::named,
    click_choice::ClickOptions,
    done::Done,
    locate::refreshed,
    mouse_click::{click_by_mouse, click_in_place},
    option_search::{Combo, LISTED_NAMES, Search, in_subtree, search},
    select_check::{item_outcome, mouse_outcome},
    settle,
    target::Prepared,
};
use crate::{call_scope::CallScope, engine::Engine, until::until, watch::Watch};

const METHOD: &str = "mouse-select";

pub fn select(engine: &Engine, prepared: &Prepared, id: ElementRef, option: &str, call: &CallScope) -> Result<Done> {
    engine.guard.before_input(Some(&prepared.window), call)?;
    let target = refreshed(engine, id)?;
    if !needs_opening(&target) {
        return pick_listed(engine, prepared, id, option, call);
    }
    let combo = Combo { window: prepared.window.clone(), id, opened_at: Instant::now() };
    open(engine, prepared, &target, call)?;
    let found = await_option(engine, &combo, option, call)?;
    let Some(item) = found.found else {
        close_dropdown(engine, prepared, id, found.popup, call)?;
        return Err(missing(id, option, &found.names));
    };
    click_in_place(engine, &item.prepared(), ClickOptions::default(), call)?;
    settle(engine, &prepared.window, call);
    let note = mouse_outcome(&refreshed(engine, id)?, option)?;
    Ok(Done::by(METHOD).with_note(note))
}

pub fn needs_opening(element: &Element) -> bool {
    element.role == Role::Combobox || element.patterns.expand_collapse
}

pub fn should_escape(expanded: Option<bool>, popup_showing: bool) -> bool {
    expanded.unwrap_or(popup_showing)
}

pub fn missing(id: ElementRef, option: &str, names: &[String]) -> Error {
    if names.is_empty() {
        return Error::not_found(format!("no option \"{option}\" is showing in {id}; no options are showing"));
    }
    let listed: Vec<String> = names.iter().take(LISTED_NAMES).map(|name| format!("\"{name}\"")).collect();
    let more = if names.len() > LISTED_NAMES { ", …" } else { "" };
    Error::not_found(format!("no option \"{option}\" is showing in {id}; showing: {}{more}", listed.join(", ")))
}

fn pick_listed(engine: &Engine, prepared: &Prepared, id: ElementRef, option: &str, call: &CallScope) -> Result<Done> {
    let found = in_subtree(engine, &prepared.window, id, option)?;
    let Some(item) = found.found else { return Err(missing(id, option, &found.names)) };
    click_by_mouse(engine, &item.prepared(), ClickOptions::default(), call)?;
    settle(engine, &prepared.window, call);
    let note = item_outcome(&refreshed(engine, item.element.id)?)?;
    Ok(Done::by(METHOD).with_note(note))
}

fn open(engine: &Engine, prepared: &Prepared, target: &Element, call: &CallScope) -> Result<()> {
    if target.state.expanded != Some(true) {
        click_by_mouse(engine, prepared, ClickOptions::default(), call)?;
    }
    settle(engine, &prepared.window, call);
    Ok(())
}

fn await_option(engine: &Engine, combo: &Combo, option: &str, call: &CallScope) -> Result<Search> {
    let patience = Duration::from_millis(engine.config.act.auto_wait_ms);
    let mut latest = Search::default();
    let found = until(engine, call, patience, &Watch::window(&combo.window), || {
        let attempt = search(engine, combo, option)?;
        if attempt.found.is_some() {
            return Ok(Some(attempt));
        }
        latest = attempt;
        Ok(None)
    })?;
    Ok(found.unwrap_or(latest))
}

fn close_dropdown(
    engine: &Engine,
    prepared: &Prepared,
    id: ElementRef,
    popup: Option<WindowRef>,
    call: &CallScope,
) -> Result<()> {
    engine.guard.check_running(call)?;
    let expanded = refreshed(engine, id).ok().and_then(|element| element.state.expanded);
    if !should_escape(expanded, popup.is_some_and(popup_showing)) {
        return Ok(());
    }
    deckhand_input::press_chord(&named(NamedKey::Escape))?;
    settle(engine, &prepared.window, call);
    Ok(())
}

fn popup_showing(hwnd: WindowRef) -> bool {
    deckhand_desktop::all_visible_windows().is_ok_and(|windows| windows.iter().any(|window| window.hwnd == hwnd))
}

#[cfg(test)]
mod tests {
    use deckhand_core::{ElementState, ErrorCode, Patterns, Rect};

    use super::*;

    fn element(role: Role, patterns: Patterns) -> Element {
        Element {
            id: ElementRef(7),
            depth: 1,
            role,
            name: "Colour".to_string(),
            value: None,
            automation_id: String::new(),
            class_name: String::new(),
            rect: Rect::new(0, 0, 10, 10),
            state: ElementState::default(),
            patterns,
        }
    }

    #[test]
    fn comboboxes_and_expanders_are_opened_first() {
        assert!(needs_opening(&element(Role::Combobox, Patterns::default())));
        assert!(needs_opening(&element(Role::Button, Patterns { expand_collapse: true, ..Patterns::default() })));
        assert!(!needs_opening(&element(Role::List, Patterns { scroll: true, ..Patterns::default() })));
    }

    #[test]
    fn escape_only_closes_a_dropdown_that_is_still_open() {
        assert!(should_escape(Some(true), false));
        assert!(!should_escape(Some(false), true));
        assert!(should_escape(None, true));
        assert!(!should_escape(None, false));
    }

    #[test]
    fn a_missing_option_lists_what_is_showing() {
        let names: Vec<String> = ["Red", "Green", "Blue"].map(String::from).to_vec();
        let error = missing(ElementRef(7), "Purple", &names);
        assert_eq!(error.code, ErrorCode::NotFound);
        assert_eq!(error.message, "no option \"Purple\" is showing in e7; showing: \"Red\", \"Green\", \"Blue\"");
        let empty = missing(ElementRef(7), "Purple", &[]);
        assert_eq!(empty.message, "no option \"Purple\" is showing in e7; no options are showing");
    }

    #[test]
    fn long_option_lists_are_cut_short() {
        let names: Vec<String> = (0..11).map(|index| format!("Item {index}")).collect();
        let message = missing(ElementRef(7), "Item 99", &names).message;
        assert!(message.ends_with("\"Item 9\", …"), "{message}");
        assert!(!message.contains("\"Item 10\""));
    }

    #[test]
    fn a_missing_popup_is_not_showing() {
        assert!(!popup_showing(WindowRef(0)));
    }
}
