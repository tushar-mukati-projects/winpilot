use std::{thread, time::Duration};

use deckhand_core::{ElementRef, Error, Result};
use windows::Win32::UI::Accessibility::{
    ExpandCollapseState_Collapsed, ExpandCollapseState_Expanded, ExpandCollapseState_PartiallyExpanded,
    IUIAutomationCondition, IUIAutomationElement, IUIAutomationExpandCollapsePattern, IUIAutomationInvokePattern,
    IUIAutomationSelectionItemPattern, PropertyConditionFlags_IgnoreCase, TreeScope_Descendants,
    UIA_ControlTypePropertyId, UIA_DataItemControlTypeId, UIA_ExpandCollapsePatternId, UIA_InvokePatternId,
    UIA_IsSelectionItemPatternAvailablePropertyId, UIA_ListItemControlTypeId, UIA_MenuItemControlTypeId,
    UIA_NamePropertyId, UIA_PROPERTY_ID, UIA_SelectionItemPatternId, UIA_TreeItemControlTypeId,
};

use crate::{
    automation::Uia,
    failure::{UiaResult, optional},
    pattern::current_pattern,
    properties::cached_text,
    resolved::Resolved,
    tree::array_elements,
    variant::OwnedVariant,
};

const EXPAND_SETTLE: Duration = Duration::from_millis(100);
const LISTED_OPTIONS: usize = 10;

impl Uia {
    pub fn select_option(&mut self, id: ElementRef, option: &str) -> Result<()> {
        let target = self.resolve(id)?;
        let opened_here = expand_if_collapsed(&target)?;
        let picked = self.pick_option(&target, option, opened_here.is_some());
        if let Some(pattern) = &opened_here {
            let _closing_is_best_effort = collapse_if_expanded(pattern, &target);
        }
        picked
    }

    fn pick_option(&self, target: &Resolved, option: &str, just_opened: bool) -> Result<()> {
        let Some(item) = self.find_option_patiently(target, option, just_opened)? else {
            return Err(self.missing_option(target, option));
        };
        choose(&item, option, target)
    }

    fn find_option_patiently(
        &self,
        target: &Resolved,
        option: &str,
        just_opened: bool,
    ) -> Result<Option<IUIAutomationElement>> {
        let condition = self.option_condition(option)?;
        let found = find_first(target, &condition)?;
        if found.is_some() || !just_opened {
            return Ok(found);
        }
        thread::sleep(EXPAND_SETTLE);
        find_first(target, &condition)
    }

    fn option_condition(&self, option: &str) -> Result<IUIAutomationCondition> {
        let name = OwnedVariant::text(option);
        let by_name = unsafe {
            self.automation.CreatePropertyConditionEx(UIA_NamePropertyId, name.raw(), PropertyConditionFlags_IgnoreCase)
        }
        .uia("building the option search")?;
        unsafe { self.automation.CreateAndCondition(&self.item_condition()?, &by_name) }
            .uia("building the option search")
    }

    fn missing_option(&self, target: &Resolved, option: &str) -> Error {
        let names = self.option_names(target).unwrap_or_default();
        Error::not_found(format!("no option \"{option}\" in {}; {}", target.label(), describe_options(&names)))
    }

    fn option_names(&self, target: &Resolved) -> Result<Vec<String>> {
        let condition = self.item_condition()?;
        let found = optional(unsafe {
            target.element.FindAllBuildCache(TreeScope_Descendants, &condition, &self.requests.element_name)
        })
        .uia(&target.doing("listing the options of"))?;
        let Some(found) = found else { return Ok(Vec::new()) };
        Ok(array_elements(&found)
            .iter()
            .map(|item| cached_text(unsafe { item.CachedName() }))
            .filter(|name| !name.trim().is_empty())
            .take(LISTED_OPTIONS + 1)
            .collect())
    }

    fn item_condition(&self) -> Result<IUIAutomationCondition> {
        let selectable = OwnedVariant::flag(true);
        let mut alternatives =
            vec![Some(self.property_condition(UIA_IsSelectionItemPatternAvailablePropertyId, &selectable)?)];
        for kind in
            [UIA_ListItemControlTypeId, UIA_MenuItemControlTypeId, UIA_TreeItemControlTypeId, UIA_DataItemControlTypeId]
        {
            alternatives.push(Some(self.property_condition(UIA_ControlTypePropertyId, &OwnedVariant::int(kind.0))?));
        }
        let any_item = unsafe { self.automation.CreateOrConditionFromNativeArray(&alternatives) }
            .uia("building the option list query")?;
        unsafe { self.automation.CreateAndCondition(&self.control_view, &any_item) }
            .uia("building the option list query")
    }

    fn property_condition(&self, property: UIA_PROPERTY_ID, value: &OwnedVariant) -> Result<IUIAutomationCondition> {
        unsafe { self.automation.CreatePropertyCondition(property, value.raw()) }.uia("building the option list query")
    }
}

fn find_first(target: &Resolved, condition: &IUIAutomationCondition) -> Result<Option<IUIAutomationElement>> {
    optional(unsafe { target.element.FindFirst(TreeScope_Descendants, condition) })
        .uia(&target.doing("searching the options of"))
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
    if state != ExpandCollapseState_Expanded && state != ExpandCollapseState_PartiallyExpanded {
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
    use deckhand_core::{ErrorCode, Role};

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
    #[ignore = "acts on the deckhand fixture app"]
    fn picks_combobox_options_by_name() {
        let Some(fixture) = launch_fixture() else { return };
        let window = fixture.window.clone();
        let handle = crate::test_support::handle();
        handle
            .run(move |uia| {
                let snapshot = uia.snapshot(&window, &SnapshotOptions::default())?;
                let combo = snapshot
                    .elements
                    .iter()
                    .find(|element| element.automation_id == "2007" && element.role == Role::Combobox)
                    .map(|element| element.id)
                    .ok_or_else(|| Error::not_found("combobox 2007"))?;
                uia.select_option(combo, "green")?;
                assert_eq!(uia.value(combo)?.as_deref(), Some("Green"));
                let missing = uia.select_option(combo, "Purple").unwrap_err();
                assert_eq!(missing.code, ErrorCode::NotFound);
                assert!(missing.message.contains("\"Red\""), "{}", missing.message);
                assert_eq!(uia.element(combo)?.state.expanded, Some(false));
                Ok(())
            })
            .unwrap();
    }
}
