use windows::Win32::UI::Accessibility::{IUIAutomationElement, UIA_PATTERN_ID};
use windows_core::Interface;
use winpilot_core::Result;

use crate::failure::{is_pattern_absent, uia_error};

pub(crate) fn current_pattern<T: Interface>(
    element: &IUIAutomationElement,
    id: UIA_PATTERN_ID,
    what: &str,
) -> Result<Option<T>> {
    match unsafe { element.GetCurrentPatternAs::<T>(id) } {
        Ok(pattern) => Ok(Some(pattern)),
        Err(error) if is_pattern_absent(&error) => Ok(None),
        Err(error) => Err(uia_error(what, &error)),
    }
}
