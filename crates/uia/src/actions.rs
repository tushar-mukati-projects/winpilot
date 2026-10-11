use deckhand_core::{ElementRef, Error, Result};
use windows::Win32::UI::Accessibility::{
    IUIAutomationExpandCollapsePattern, IUIAutomationInvokePattern, IUIAutomationLegacyIAccessiblePattern,
    IUIAutomationScrollItemPattern, IUIAutomationSelectionItemPattern, IUIAutomationValuePattern,
    UIA_ExpandCollapsePatternId, UIA_InvokePatternId, UIA_LegacyIAccessiblePatternId, UIA_ScrollItemPatternId,
    UIA_SelectionItemPatternId, UIA_ValuePatternId,
};
use windows_core::BSTR;

use crate::{automation::Uia, failure::UiaResult, pattern::current_pattern};

impl Uia {
    pub fn invoke(&mut self, id: ElementRef) -> Result<()> {
        let target = self.resolve(id)?;
        let pattern: IUIAutomationInvokePattern = target.pattern(UIA_InvokePatternId, "Invoke")?;
        unsafe { pattern.Invoke() }.uia(&target.doing("invoking"))
    }

    pub fn select(&mut self, id: ElementRef) -> Result<()> {
        let target = self.resolve(id)?;
        let pattern: IUIAutomationSelectionItemPattern = target.pattern(UIA_SelectionItemPatternId, "SelectionItem")?;
        unsafe { pattern.Select() }.uia(&target.doing("selecting"))
    }

    pub fn deselect(&mut self, id: ElementRef) -> Result<()> {
        let target = self.resolve(id)?;
        let pattern: IUIAutomationSelectionItemPattern = target.pattern(UIA_SelectionItemPatternId, "SelectionItem")?;
        unsafe { pattern.RemoveFromSelection() }.uia(&target.doing("deselecting"))
    }

    pub fn expand(&mut self, id: ElementRef) -> Result<()> {
        let target = self.resolve(id)?;
        let pattern: IUIAutomationExpandCollapsePattern =
            target.pattern(UIA_ExpandCollapsePatternId, "ExpandCollapse")?;
        unsafe { pattern.Expand() }.uia(&target.doing("expanding"))
    }

    pub fn collapse(&mut self, id: ElementRef) -> Result<()> {
        let target = self.resolve(id)?;
        let pattern: IUIAutomationExpandCollapsePattern =
            target.pattern(UIA_ExpandCollapsePatternId, "ExpandCollapse")?;
        unsafe { pattern.Collapse() }.uia(&target.doing("collapsing"))
    }

    pub fn set_value(&mut self, id: ElementRef, value: &str) -> Result<()> {
        let target = self.resolve(id)?;
        let pattern: IUIAutomationValuePattern = target.pattern(UIA_ValuePatternId, "Value")?;
        if target.info.state.read_only {
            return Err(Error::unsupported(format!("{} is read-only", target.label())));
        }
        unsafe { pattern.SetValue(&BSTR::from(value)) }.uia(&target.doing("setting the value of"))
    }

    pub fn value(&mut self, id: ElementRef) -> Result<Option<String>> {
        let target = self.resolve(id)?;
        if target.info.state.password {
            return Ok(None);
        }
        let reading = target.doing("reading the value of");
        let Some(pattern) =
            current_pattern::<IUIAutomationValuePattern>(&target.element, UIA_ValuePatternId, &reading)?
        else {
            return Ok(None);
        };
        let value = unsafe { pattern.CurrentValue() }.uia(&reading)?;
        Ok(Some(value.to_string()))
    }

    pub fn set_focus(&mut self, id: ElementRef) -> Result<()> {
        let target = self.resolve(id)?;
        unsafe { target.element.SetFocus() }.uia(&target.doing("focusing"))
    }

    pub fn scroll_into_view(&mut self, id: ElementRef) -> Result<()> {
        let target = self.resolve(id)?;
        let pattern: IUIAutomationScrollItemPattern = target.pattern(UIA_ScrollItemPatternId, "ScrollItem")?;
        unsafe { pattern.ScrollIntoView() }.uia(&target.doing("scrolling into view"))
    }

    pub fn default_action(&mut self, id: ElementRef) -> Result<()> {
        let target = self.resolve(id)?;
        let pattern: IUIAutomationLegacyIAccessiblePattern =
            target.pattern(UIA_LegacyIAccessiblePatternId, "LegacyIAccessible")?;
        unsafe { pattern.DoDefaultAction() }.uia(&target.doing("running the default action of"))
    }
}

#[cfg(test)]
mod tests {
    use deckhand_core::{Element, ErrorCode, Role, Snapshot};

    use super::*;
    use crate::{
        SnapshotOptions,
        test_support::{handle, launch_fixture},
    };

    fn control(snapshot: &Snapshot, automation_id: &str, role: Role) -> Result<ElementRef> {
        snapshot
            .elements
            .iter()
            .find(|element: &&Element| element.automation_id == automation_id && element.role == role)
            .map(|element| element.id)
            .ok_or_else(|| Error::not_found(format!("fixture control {automation_id}")))
    }

    #[test]
    #[ignore = "acts on the deckhand fixture app"]
    fn sets_and_reads_the_edit_value() {
        let Some(fixture) = launch_fixture() else { return };
        let window = fixture.window.clone();
        handle()
            .run(move |uia| {
                let snapshot = uia.snapshot(&window, &SnapshotOptions::default())?;
                let edit = control(&snapshot, "2001", Role::Edit)?;
                uia.set_value(edit, "hello deckhand")?;
                assert_eq!(uia.value(edit)?.as_deref(), Some("hello deckhand"));
                assert_eq!(uia.text(edit, 5)?, "hello");
                assert_eq!(uia.element(edit)?.value.as_deref(), Some("hello deckhand"));
                assert!(uia.contains(snapshot.elements[0].id, edit)?);
                assert_eq!(uia.window_of(edit)?, window.hwnd);
                Ok(())
            })
            .unwrap();
    }

    #[test]
    #[ignore = "acts on the deckhand fixture app"]
    fn invokes_the_button_and_toggles_the_checkbox() {
        let Some(fixture) = launch_fixture() else { return };
        let window = fixture.window.clone();
        handle()
            .run(move |uia| {
                let snapshot = uia.snapshot(&window, &SnapshotOptions::default())?;
                let button = control(&snapshot, "2003", Role::Button)?;
                let checkbox = control(&snapshot, "2005", Role::Checkbox)?;
                uia.invoke(button)?;
                let first = uia.toggle(checkbox)?;
                let second = uia.toggle(checkbox)?;
                assert_ne!(first, second);
                assert_eq!(uia.element(checkbox)?.state.toggled, Some(second));
                let missing = uia.toggle(button).unwrap_err();
                assert_eq!(missing.code, ErrorCode::Unsupported);
                assert!(missing.message.contains("has no Toggle pattern"), "{}", missing.message);
                Ok(())
            })
            .unwrap();
    }

    #[test]
    #[ignore = "acts on the deckhand fixture app"]
    fn selects_a_listbox_item() {
        let Some(fixture) = launch_fixture() else { return };
        let window = fixture.window.clone();
        handle()
            .run(move |uia| {
                let snapshot = uia.snapshot(&window, &SnapshotOptions::default())?;
                let list = control(&snapshot, "2009", Role::List)?;
                let index = snapshot.index_of(list).ok_or_else(|| Error::not_found("listbox index"))?;
                let item = snapshot
                    .descendants(index)
                    .iter()
                    .find(|element| element.role == Role::ListItem)
                    .map(|element| element.id)
                    .ok_or_else(|| Error::not_found("a listbox item"))?;
                uia.select(item)?;
                assert_eq!(uia.element(item)?.state.selected, Some(true));
                assert!(uia.contains(list, item)?);
                Ok(())
            })
            .unwrap();
    }
}
