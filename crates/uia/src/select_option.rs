use std::{thread, time::Duration};

use deckhand_core::{ElementRef, Error, Result, Role};
use windows::Win32::UI::Accessibility::{
    ExpandCollapseState, ExpandCollapseState_Collapsed, ExpandCollapseState_Expanded,
    ExpandCollapseState_PartiallyExpanded, IUIAutomationElement, IUIAutomationExpandCollapsePattern,
    IUIAutomationInvokePattern, IUIAutomationSelectionItemPattern, UIA_CONTROLTYPE_ID, UIA_DataItemControlTypeId,
    UIA_ExpandCollapsePatternId, UIA_InvokePatternId, UIA_IsSelectionItemPatternAvailablePropertyId,
    UIA_ListItemControlTypeId, UIA_MenuItemControlTypeId, UIA_SelectionItemPatternId, UIA_TreeItemControlTypeId,
};

use crate::{
    automation::Uia,
    cached_tree::{CachedNode, read_tree},
    failure::UiaResult,
    pattern::current_pattern,
    properties::{cached_bool, cached_text},
    resolved::Resolved,
    win32_combo::Win32Combo,
};

const EXPAND_SETTLE: Duration = Duration::from_millis(100);
const LISTED_OPTIONS: usize = 10;
const MAX_SEARCHED_ELEMENTS: usize = 5000;
const ITEM_CONTROL_TYPES: [UIA_CONTROLTYPE_ID; 4] =
    [UIA_ListItemControlTypeId, UIA_MenuItemControlTypeId, UIA_TreeItemControlTypeId, UIA_DataItemControlTypeId];

struct ListedOption {
    element: IUIAutomationElement,
    name: String,
}

impl Uia {
    pub fn select_option(&mut self, id: ElementRef, option: &str) -> Result<()> {
        let target = self.resolve(id)?;
        let win32 = win32_combo_of(&target);
        let opened_here = expand_if_collapsed(&target)?;
        let picked = self.pick_option(&target, option, opened_here.is_some());
        let chosen = win32.as_ref().filter(|_| picked.is_ok()).and_then(Win32Combo::selected_index);
        if let Some(pattern) = &opened_here {
            let _closing_is_best_effort = collapse_if_expanded(pattern, &target);
        }
        picked?;
        let (Some(combo), Some(index)) = (win32, chosen) else { return Ok(()) };
        combo.keep_selection(index).map_err(|error| picking_error(&error, option, &target))
    }

    fn pick_option(&self, target: &Resolved, option: &str, just_opened: bool) -> Result<()> {
        let options = self.options_patiently(target, option, just_opened)?;
        let Some(item) = options.iter().find(|listed| same_option(&listed.name, option)) else {
            return Err(missing_option(target, option, &options));
        };
        choose(&item.element, option, target)
    }

    fn options_patiently(&self, target: &Resolved, option: &str, just_opened: bool) -> Result<Vec<ListedOption>> {
        let options = self.listed_options(target)?;
        if !just_opened || options.iter().any(|listed| same_option(&listed.name, option)) {
            return Ok(options);
        }
        thread::sleep(EXPAND_SETTLE);
        self.listed_options(target)
    }

    fn listed_options(&self, target: &Resolved) -> Result<Vec<ListedOption>> {
        let requests = self.requests.options();
        let root = read_tree(&target.element, requests).uia(&target.doing("listing the options of"))?;
        let mut options = Vec::new();
        let mut pending: Vec<CachedNode> = self.tree_children(&root, requests).into_iter().rev().collect();
        for _ in 0..MAX_SEARCHED_ELEMENTS {
            let Some(node) = pending.pop() else { break };
            pending.extend(self.tree_children(&node, requests).into_iter().rev());
            if is_option(&node.element) {
                let name = cached_text(unsafe { node.element.CachedName() });
                options.push(ListedOption { element: node.element, name });
            }
        }
        Ok(options)
    }
}

fn is_option(element: &IUIAutomationElement) -> bool {
    cached_bool(element, UIA_IsSelectionItemPatternAvailablePropertyId)
        || unsafe { element.CachedControlType() }.is_ok_and(|kind| ITEM_CONTROL_TYPES.contains(&kind))
}

fn same_option(name: &str, option: &str) -> bool {
    name.to_lowercase() == option.to_lowercase()
}

fn missing_option(target: &Resolved, option: &str, options: &[ListedOption]) -> Error {
    let names: Vec<String> = options
        .iter()
        .map(|listed| listed.name.trim())
        .filter(|name| !name.is_empty())
        .take(LISTED_OPTIONS + 1)
        .map(str::to_string)
        .collect();
    Error::not_found(format!("no option \"{option}\" in {}; {}", target.label(), describe_options(&names)))
}

fn win32_combo_of(target: &Resolved) -> Option<Win32Combo> {
    if target.info.role != Role::Combobox {
        return None;
    }
    Win32Combo::of(&target.element)
}

fn picking_error(error: &Error, option: &str, target: &Resolved) -> Error {
    Error::new(error.code, format!("picking \"{option}\" in {}: {}", target.label(), error.message))
}

fn choose(item: &IUIAutomationElement, option: &str, target: &Resolved) -> Result<()> {
    let action = format!("picking \"{option}\" in {}", target.label());
    let selectable = current_pattern::<IUIAutomationSelectionItemPattern>(item, UIA_SelectionItemPatternId, &action)?;
    if let Some(pattern) = selectable {
        return unsafe { pattern.Select() }.uia(&action);
    }
    if let Some(pattern) = current_pattern::<IUIAutomationInvokePattern>(item, UIA_InvokePatternId, &action)? {
        return unsafe { pattern.Invoke() }.uia(&action);
    }
    Err(Error::unsupported(format!("option \"{option}\" in {} can be neither selected nor invoked", target.label())))
}

fn is_open(state: ExpandCollapseState) -> bool {
    state == ExpandCollapseState_Expanded || state == ExpandCollapseState_PartiallyExpanded
}

fn expand_if_collapsed(target: &Resolved) -> Result<Option<IUIAutomationExpandCollapsePattern>> {
    let reading = target.doing("reading the expand state of");
    let pattern =
        current_pattern::<IUIAutomationExpandCollapsePattern>(&target.element, UIA_ExpandCollapsePatternId, &reading)?;
    let Some(pattern) = pattern else { return Ok(None) };
    let state = unsafe { pattern.CurrentExpandCollapseState() }.uia(&reading)?;
    if state != ExpandCollapseState_Collapsed {
        return Ok(None);
    }
    unsafe { pattern.Expand() }.uia(&target.doing("expanding"))?;
    Ok(Some(pattern))
}

fn collapse_if_expanded(pattern: &IUIAutomationExpandCollapsePattern, target: &Resolved) -> Result<()> {
    let state = unsafe { pattern.CurrentExpandCollapseState() }.uia(&target.doing("reading the expand state of"))?;
    if !is_open(state) {
        return Ok(());
    }
    unsafe { pattern.Collapse() }.uia(&target.doing("collapsing"))
}

fn describe_options(names: &[String]) -> String {
    if names.is_empty() {
        return "it lists no options".to_string();
    }
    let listed: Vec<String> = names.iter().take(LISTED_OPTIONS).map(|name| format!("\"{name}\"")).collect();
    let more = if names.len() > LISTED_OPTIONS { ", …" } else { "" };
    format!("options: {}{more}", listed.join(", "))
}

#[cfg(test)]
mod tests {
    use deckhand_core::ErrorCode;

    use super::*;
    use crate::{SnapshotOptions, test_support::launch_fixture};

    #[test]
    fn describes_missing_options() {
        assert_eq!(describe_options(&[]), "it lists no options");
        let three: Vec<String> = ["Red", "Green", "Blue"].map(String::from).to_vec();
        assert_eq!(describe_options(&three), "options: \"Red\", \"Green\", \"Blue\"");
        let many: Vec<String> = (0..11).map(|index| index.to_string()).collect();
        let text = describe_options(&many);
        assert!(text.ends_with("\"9\", …"));
        assert!(!text.contains("\"10\""));
    }

    #[test]
    fn options_match_by_whole_name_ignoring_case() {
        assert!(same_option("Green", "green"));
        assert!(same_option("ÉCLAIR", "éclair"));
        assert!(!same_option("Greenish", "green"));
        assert!(!same_option("Green", "gree"));
    }

    #[test]
    fn only_open_states_count_as_expanded() {
        assert!(is_open(ExpandCollapseState_Expanded));
        assert!(is_open(ExpandCollapseState_PartiallyExpanded));
        assert!(!is_open(ExpandCollapseState_Collapsed));
    }

    #[test]
    #[ignore = "acts on the deckhand fixture app"]
    fn picks_combobox_options_by_name() {
        let Some(fixture) = launch_fixture() else { return };
        let window = fixture.window.clone();
        let handle = crate::test_support::handle();
        let (value, missing, expanded) = handle
            .run(move |uia| {
                let snapshot = uia.snapshot(&window, &SnapshotOptions::default())?;
                let combo = snapshot
                    .elements
                    .iter()
                    .find(|element| element.automation_id == "2007" && element.role == Role::Combobox)
                    .map(|element| element.id)
                    .ok_or_else(|| Error::not_found("combobox 2007"))?;
                uia.select_option(combo, "green")?;
                let value = uia.value(combo)?;
                let missing = uia.select_option(combo, "Purple");
                Ok((value, missing, uia.element(combo)?.state.expanded))
            })
            .unwrap();
        assert_eq!(value.as_deref(), Some("Green"));
        let missing = missing.unwrap_err();
        assert_eq!(missing.code, ErrorCode::NotFound);
        assert!(missing.message.contains("options: \"Red\", \"Green\", \"Blue\""), "{}", missing.message);
        assert_eq!(expanded, Some(false));
    }
}
