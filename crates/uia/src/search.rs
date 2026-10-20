use std::time::Duration;

use windows::Win32::UI::Accessibility::{
    IUIAutomationCondition, PropertyConditionFlags, PropertyConditionFlags_IgnoreCase,
    PropertyConditionFlags_MatchSubstring, TreeScope_Descendants, UIA_AppBarControlTypeId, UIA_ButtonControlTypeId,
    UIA_ControlTypePropertyId, UIA_NamePropertyId,
};
use winpilot_core::{Element, Result, Role, WindowRef, selector::SearchHint};
use winpilot_win32::hwnd_of;

use crate::{
    automation::Uia,
    cached_tree::array_elements,
    failure::{UiaResult, optional},
    variant::OwnedVariant,
};

const KEPT_MATCHES: usize = 50;
const EXACT_NAME: PropertyConditionFlags = PropertyConditionFlags_IgnoreCase;
const NAME_PART: PropertyConditionFlags =
    PropertyConditionFlags(PropertyConditionFlags_IgnoreCase.0 | PropertyConditionFlags_MatchSubstring.0);

impl Uia {
    pub fn search(&mut self, window: WindowRef, hint: &SearchHint, limit: Duration) -> Result<Vec<Element>> {
        let doing = format!("searching window {window} for \"{}\"", hint.name);
        let root = unsafe { self.automation.ElementFromHandle(hwnd_of(window)) }.uia(&doing)?;
        let Some(condition) = self.condition_for(hint).uia(&doing)? else { return Ok(Vec::new()) };
        let found = self.within(limit, &doing, || {
            optional(unsafe {
                root.FindAllBuildCache(TreeScope_Descendants, &condition, &self.requests.onscreen_element_props)
            })
        })?;
        let Some(found) = found else { return Ok(Vec::new()) };
        Ok(array_elements(&found)
            .into_iter()
            .take(KEPT_MATCHES)
            .map(|element| self.adopt_in(element, window))
            .collect())
    }

    fn condition_for(&self, hint: &SearchHint) -> windows::core::Result<Option<IUIAutomationCondition>> {
        let name = OwnedVariant::text(&hint.name);
        let flags = if hint.exact { EXACT_NAME } else { NAME_PART };
        let named = unsafe { self.automation.CreatePropertyConditionEx(UIA_NamePropertyId, name.raw(), flags) }?;
        let Some(role) = hint.role else { return Ok(Some(named)) };
        let types: Vec<Option<IUIAutomationCondition>> = control_types_of(role)
            .into_iter()
            .map(|id| {
                let value = OwnedVariant::int(id);
                unsafe { self.automation.CreatePropertyCondition(UIA_ControlTypePropertyId, value.raw()) }.map(Some)
            })
            .collect::<windows::core::Result<_>>()?;
        if types.is_empty() {
            return Ok(None);
        }
        let any_type = unsafe { self.automation.CreateOrConditionFromNativeArray(&types) }?;
        unsafe { self.automation.CreateAndCondition(&any_type, &named) }.map(Some)
    }
}

fn control_types_of(role: Role) -> Vec<i32> {
    (UIA_ButtonControlTypeId.0..=UIA_AppBarControlTypeId.0).filter(|&id| Role::from_control_type(id) == role).collect()
}

#[cfg(test)]
mod tests {
    use winpilot_core::Context;

    use super::*;
    use crate::{
        SnapshotOptions,
        test_support::{handle, taskbar},
    };

    const SEARCH_LIMIT: Duration = Duration::from_secs(2);

    #[test]
    fn roles_search_by_the_control_types_that_map_to_them() {
        assert_eq!(control_types_of(Role::Button), [UIA_ButtonControlTypeId.0]);
        for id in UIA_ButtonControlTypeId.0..=UIA_AppBarControlTypeId.0 {
            assert!(control_types_of(Role::from_control_type(id)).contains(&id), "{id}");
        }
    }

    #[test]
    #[ignore = "reads the user's windows"]
    fn a_search_finds_a_taskbar_element_under_its_snapshot_ref() {
        let window = taskbar();
        let hwnd = window.hwnd;
        let (named, found) = handle()
            .run(move |uia| {
                let snapshot = uia.snapshot(&window, &SnapshotOptions::default())?;
                let named = snapshot
                    .elements
                    .iter()
                    .skip(1)
                    .find(|element| element.is_visible() && !element.name.trim().is_empty())
                    .cloned()
                    .context("a visible, named taskbar element")?;
                let hint = SearchHint { role: Some(named.role), name: named.name.clone(), exact: true };
                Ok((named, uia.search(hwnd, &hint, SEARCH_LIMIT)?))
            })
            .unwrap();
        assert!(found.iter().any(|element| element.id == named.id), "{named:?} not in {found:?}");
    }
}
