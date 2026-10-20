use windows::Win32::UI::Accessibility::{
    AutomationElementMode_Full, IUIAutomation, IUIAutomationCacheRequest, IUIAutomationCondition, TreeScope,
    TreeScope_Children, TreeScope_Element, TreeScope_Subtree, UIA_ControlTypePropertyId, UIA_IsOffscreenPropertyId,
    UIA_IsSelectionItemPatternAvailablePropertyId, UIA_NamePropertyId, UIA_NativeWindowHandlePropertyId,
    UIA_PROPERTY_ID, UIA_RuntimeIdPropertyId,
};
use winpilot_core::{Context, Result};

use crate::{properties::ELEMENT_PROPERTIES, tree_path::View, variant::OwnedVariant};

const ELEMENT_AND_CHILDREN: TreeScope = TreeScope(TreeScope_Element.0 | TreeScope_Children.0);

const NAME_PROPERTIES: [UIA_PROPERTY_ID; 3] =
    [UIA_NamePropertyId, UIA_NativeWindowHandlePropertyId, UIA_RuntimeIdPropertyId];

const OPTION_PROPERTIES: [UIA_PROPERTY_ID; 5] = [
    UIA_NamePropertyId,
    UIA_ControlTypePropertyId,
    UIA_IsSelectionItemPatternAvailablePropertyId,
    UIA_NativeWindowHandlePropertyId,
    UIA_RuntimeIdPropertyId,
];

pub(crate) struct CacheRequests {
    props: TreeRequests,
    onscreen_props: TreeRequests,
    pub(crate) names: TreeRequests,
    pub(crate) options: TreeRequests,
    pub(crate) element_props: IUIAutomationCacheRequest,
    pub(crate) onscreen_element_props: IUIAutomationCacheRequest,
    pub(crate) element_hwnd: IUIAutomationCacheRequest,
    pub(crate) children_ids: IUIAutomationCacheRequest,
}

impl CacheRequests {
    pub(crate) fn build(automation: &IUIAutomation, control_view: &IUIAutomationCondition) -> Result<Self> {
        let raw_view = unsafe { automation.RawViewCondition() }.context("creating the raw view condition")?;
        let onscreen_view = onscreen_controls(automation, control_view)?;
        let handle = [UIA_NativeWindowHandlePropertyId];
        let runtime_id = [UIA_RuntimeIdPropertyId];
        Ok(Self {
            props: TreeRequests::build(automation, &ELEMENT_PROPERTIES, control_view)?,
            onscreen_props: TreeRequests::build(automation, &ELEMENT_PROPERTIES, &onscreen_view)?,
            names: TreeRequests::build(automation, &NAME_PROPERTIES, control_view)?,
            options: TreeRequests::build(automation, &OPTION_PROPERTIES, control_view)?,
            element_props: live(automation, &ELEMENT_PROPERTIES, TreeScope_Element, &raw_view)?,
            onscreen_element_props: live(automation, &ELEMENT_PROPERTIES, TreeScope_Element, &onscreen_view)?,
            element_hwnd: live(automation, &handle, TreeScope_Element, &raw_view)?,
            children_ids: live(automation, &runtime_id, ELEMENT_AND_CHILDREN, control_view)?,
        })
    }

    pub(crate) fn props_in(&self, view: View) -> &TreeRequests {
        match view {
            View::Onscreen => &self.onscreen_props,
            View::Full => &self.props,
        }
    }
}

pub(crate) struct TreeRequests {
    pub(crate) whole: IUIAutomationCacheRequest,
    pub(crate) level: IUIAutomationCacheRequest,
}

impl TreeRequests {
    fn build(
        automation: &IUIAutomation,
        properties: &[UIA_PROPERTY_ID],
        filter: &IUIAutomationCondition,
    ) -> Result<Self> {
        Ok(Self {
            whole: live(automation, properties, TreeScope_Subtree, filter)?,
            level: live(automation, properties, ELEMENT_AND_CHILDREN, filter)?,
        })
    }
}

fn onscreen_controls(
    automation: &IUIAutomation,
    control_view: &IUIAutomationCondition,
) -> Result<IUIAutomationCondition> {
    let false_value = OwnedVariant::flag(false);
    let not_offscreen = unsafe { automation.CreatePropertyCondition(UIA_IsOffscreenPropertyId, false_value.raw()) }
        .context("creating the on-screen condition")?;
    unsafe { automation.CreateAndCondition(control_view, &not_offscreen) }.context("creating the on-screen view")
}

fn live(
    automation: &IUIAutomation,
    properties: &[UIA_PROPERTY_ID],
    scope: TreeScope,
    filter: &IUIAutomationCondition,
) -> Result<IUIAutomationCacheRequest> {
    let request = unsafe { automation.CreateCacheRequest() }.context("creating a UI Automation cache request")?;
    for property in properties {
        unsafe { request.AddProperty(*property) }.context("adding a property to a cache request")?;
    }
    unsafe { request.SetTreeScope(scope) }.context("setting a cache request's tree scope")?;
    unsafe { request.SetTreeFilter(filter) }.context("setting a cache request's tree filter")?;
    unsafe { request.SetAutomationElementMode(AutomationElementMode_Full) }
        .context("setting a cache request's element mode")?;
    Ok(request)
}
