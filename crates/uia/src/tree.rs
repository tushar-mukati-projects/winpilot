use windows::Win32::UI::Accessibility::{IUIAutomationElement, IUIAutomationElementArray};

pub(crate) fn cached_children(element: &IUIAutomationElement) -> Vec<IUIAutomationElement> {
    unsafe { element.GetCachedChildren() }.map(|children| array_elements(&children)).unwrap_or_default()
}

fn array_elements(array: &IUIAutomationElementArray) -> Vec<IUIAutomationElement> {
    let count = unsafe { array.Length() }.unwrap_or(0);
    (0..count).filter_map(|index| unsafe { array.GetElement(index) }.ok()).collect()
}
