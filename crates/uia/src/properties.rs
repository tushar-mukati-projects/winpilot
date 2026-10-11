use deckhand_core::{Element, ElementRef, ElementState, Patterns, Rect, Role, WindowRef};
use windows::Win32::{
    Foundation::RECT,
    UI::Accessibility::{
        ExpandCollapseState, ExpandCollapseState_Collapsed, ExpandCollapseState_Expanded,
        ExpandCollapseState_PartiallyExpanded, IUIAutomationElement, ToggleState, ToggleState_Off, ToggleState_On,
        UIA_AutomationIdPropertyId, UIA_BoundingRectanglePropertyId, UIA_ClassNamePropertyId,
        UIA_ControlTypePropertyId, UIA_ExpandCollapseExpandCollapseStatePropertyId, UIA_HasKeyboardFocusPropertyId,
        UIA_IsEnabledPropertyId, UIA_IsExpandCollapsePatternAvailablePropertyId,
        UIA_IsInvokePatternAvailablePropertyId, UIA_IsKeyboardFocusablePropertyId,
        UIA_IsLegacyIAccessiblePatternAvailablePropertyId, UIA_IsOffscreenPropertyId, UIA_IsPasswordPropertyId,
        UIA_IsRangeValuePatternAvailablePropertyId, UIA_IsScrollItemPatternAvailablePropertyId,
        UIA_IsScrollPatternAvailablePropertyId, UIA_IsSelectionItemPatternAvailablePropertyId,
        UIA_IsTextPatternAvailablePropertyId, UIA_IsTogglePatternAvailablePropertyId,
        UIA_IsValuePatternAvailablePropertyId, UIA_NamePropertyId, UIA_NativeWindowHandlePropertyId, UIA_PROPERTY_ID,
        UIA_ProcessIdPropertyId, UIA_RuntimeIdPropertyId, UIA_SelectionItemIsSelectedPropertyId,
        UIA_ToggleToggleStatePropertyId, UIA_ValueIsReadOnlyPropertyId, UIA_ValueValuePropertyId,
    },
};
use windows_core::{BOOL, BSTR};

use crate::{
    fingerprint::Fingerprint, text::truncate_chars, tree_path::TreePath, variant::OwnedVariant, win,
    window_identity::WindowIdentity,
};

const VALUE_LIMIT: usize = 4096;

pub(crate) const ELEMENT_PROPERTIES: [UIA_PROPERTY_ID; 28] = [
    UIA_NamePropertyId,
    UIA_ControlTypePropertyId,
    UIA_AutomationIdPropertyId,
    UIA_ClassNamePropertyId,
    UIA_BoundingRectanglePropertyId,
    UIA_IsEnabledPropertyId,
    UIA_IsOffscreenPropertyId,
    UIA_IsKeyboardFocusablePropertyId,
    UIA_HasKeyboardFocusPropertyId,
    UIA_RuntimeIdPropertyId,
    UIA_IsPasswordPropertyId,
    UIA_ProcessIdPropertyId,
    UIA_NativeWindowHandlePropertyId,
    UIA_ValueValuePropertyId,
    UIA_ValueIsReadOnlyPropertyId,
    UIA_ToggleToggleStatePropertyId,
    UIA_ExpandCollapseExpandCollapseStatePropertyId,
    UIA_SelectionItemIsSelectedPropertyId,
    UIA_IsInvokePatternAvailablePropertyId,
    UIA_IsValuePatternAvailablePropertyId,
    UIA_IsTogglePatternAvailablePropertyId,
    UIA_IsSelectionItemPatternAvailablePropertyId,
    UIA_IsExpandCollapsePatternAvailablePropertyId,
    UIA_IsScrollPatternAvailablePropertyId,
    UIA_IsScrollItemPatternAvailablePropertyId,
    UIA_IsTextPatternAvailablePropertyId,
    UIA_IsRangeValuePatternAvailablePropertyId,
    UIA_IsLegacyIAccessiblePatternAvailablePropertyId,
];

pub(crate) struct NodeProps {
    pub(crate) runtime_id: Vec<i32>,
    pub(crate) role: Role,
    pub(crate) name: String,
    pub(crate) automation_id: String,
    pub(crate) class_name: String,
    pub(crate) rect: Rect,
    pub(crate) native_window: WindowRef,
    pub(crate) value: Option<String>,
    pub(crate) state: ElementState,
    pub(crate) patterns: Patterns,
}

impl NodeProps {
    pub(crate) fn read(element: &IUIAutomationElement) -> Self {
        let patterns = read_patterns(element);
        let state = read_state(element, &patterns);
        Self {
            runtime_id: cached_variant(element, UIA_RuntimeIdPropertyId)
                .and_then(|variant| variant.as_i32_array())
                .unwrap_or_default(),
            role: Role::from_control_type(unsafe { element.CachedControlType() }.map_or(0, |kind| kind.0)),
            name: cached_text(unsafe { element.CachedName() }),
            automation_id: cached_text(unsafe { element.CachedAutomationId() }),
            class_name: cached_text(unsafe { element.CachedClassName() }),
            rect: unsafe { element.CachedBoundingRectangle() }.map(rect_from).unwrap_or_default(),
            native_window: unsafe { element.CachedNativeWindowHandle() }.map_or(WindowRef(0), win::window_ref),
            value: read_value(element, &patterns, &state),
            state,
            patterns,
        }
    }

    pub(crate) fn fingerprint(&self, window: WindowIdentity, path: Option<TreePath>) -> Fingerprint {
        Fingerprint {
            window,
            role: self.role,
            name: self.name.clone(),
            automation_id: self.automation_id.clone(),
            class_name: self.class_name.clone(),
            path,
        }
    }

    pub(crate) fn into_element(self, id: ElementRef, depth: u16) -> Element {
        Element {
            id,
            depth,
            role: self.role,
            name: self.name,
            value: self.value,
            automation_id: self.automation_id,
            class_name: self.class_name,
            rect: self.rect,
            state: self.state,
            patterns: self.patterns,
        }
    }
}

pub(crate) fn cached_text(result: windows_core::Result<BSTR>) -> String {
    result.map(|text| text.to_string()).unwrap_or_default()
}

fn cached_flag(result: windows_core::Result<BOOL>) -> bool {
    result.is_ok_and(BOOL::as_bool)
}

fn cached_variant(element: &IUIAutomationElement, property: UIA_PROPERTY_ID) -> Option<OwnedVariant> {
    unsafe { element.GetCachedPropertyValue(property) }.ok().map(OwnedVariant::from_raw)
}

fn cached_bool(element: &IUIAutomationElement, property: UIA_PROPERTY_ID) -> bool {
    cached_variant(element, property).and_then(|variant| variant.as_bool()).unwrap_or(false)
}

fn cached_i32(element: &IUIAutomationElement, property: UIA_PROPERTY_ID) -> Option<i32> {
    cached_variant(element, property).and_then(|variant| variant.as_i32())
}

fn rect_from(rect: RECT) -> Rect {
    Rect::from_edges(rect.left, rect.top, rect.right, rect.bottom)
}

fn read_patterns(element: &IUIAutomationElement) -> Patterns {
    let has = |property| cached_bool(element, property);
    Patterns {
        invoke: has(UIA_IsInvokePatternAvailablePropertyId),
        value: has(UIA_IsValuePatternAvailablePropertyId),
        toggle: has(UIA_IsTogglePatternAvailablePropertyId),
        selection_item: has(UIA_IsSelectionItemPatternAvailablePropertyId),
        expand_collapse: has(UIA_IsExpandCollapsePatternAvailablePropertyId),
        scroll: has(UIA_IsScrollPatternAvailablePropertyId),
        scroll_item: has(UIA_IsScrollItemPatternAvailablePropertyId),
        text: has(UIA_IsTextPatternAvailablePropertyId),
        range_value: has(UIA_IsRangeValuePatternAvailablePropertyId),
        legacy: has(UIA_IsLegacyIAccessiblePatternAvailablePropertyId),
    }
}

fn read_state(element: &IUIAutomationElement, patterns: &Patterns) -> ElementState {
    ElementState {
        enabled: cached_flag(unsafe { element.CachedIsEnabled() }),
        focused: cached_flag(unsafe { element.CachedHasKeyboardFocus() }),
        focusable: cached_flag(unsafe { element.CachedIsKeyboardFocusable() }),
        offscreen: cached_flag(unsafe { element.CachedIsOffscreen() }),
        read_only: patterns.value && cached_bool(element, UIA_ValueIsReadOnlyPropertyId),
        password: cached_flag(unsafe { element.CachedIsPassword() }),
        toggled: if patterns.toggle { toggled(element) } else { None },
        expanded: if patterns.expand_collapse { expanded(element) } else { None },
        selected: patterns.selection_item.then(|| cached_bool(element, UIA_SelectionItemIsSelectedPropertyId)),
    }
}

fn toggled(element: &IUIAutomationElement) -> Option<bool> {
    let state = ToggleState(cached_i32(element, UIA_ToggleToggleStatePropertyId)?);
    (state == ToggleState_On || state == ToggleState_Off).then_some(state == ToggleState_On)
}

fn expanded(element: &IUIAutomationElement) -> Option<bool> {
    let state = ExpandCollapseState(cached_i32(element, UIA_ExpandCollapseExpandCollapseStatePropertyId)?);
    let open = state == ExpandCollapseState_Expanded || state == ExpandCollapseState_PartiallyExpanded;
    (open || state == ExpandCollapseState_Collapsed).then_some(open)
}

fn read_value(element: &IUIAutomationElement, patterns: &Patterns, state: &ElementState) -> Option<String> {
    if !patterns.value || state.password {
        return None;
    }
    let value = cached_variant(element, UIA_ValueValuePropertyId)?.as_string()?;
    Some(truncate_chars(value, VALUE_LIMIT))
}
