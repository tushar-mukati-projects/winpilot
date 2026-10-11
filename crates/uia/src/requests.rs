use deckhand_core::{Context, Result};
use windows::Win32::UI::Accessibility::{
    AutomationElementMode, AutomationElementMode_Full, AutomationElementMode_None, IUIAutomation,
    IUIAutomationCacheRequest, IUIAutomationCondition, TreeScope, TreeScope_Element, TreeScope_Subtree,
    UIA_IsOffscreenPropertyId, UIA_NamePropertyId, UIA_NativeWindowHandlePropertyId, UIA_PROPERTY_ID,
};

use crate::{properties::ELEMENT_PROPERTIES, tree_path::View, variant::OwnedVariant};

pub(crate) struct CacheRequests {
    pub(crate) subtree_props: IUIAutomationCacheRequest,
    pub(crate) onscreen_subtree_props: IUIAutomationCacheRequest,
    pub(crate) element_props: IUIAutomationCacheRequest,
    pub(crate) subtree_names: IUIAutomationCacheRequest,
    pub(crate) element_name: IUIAutomationCacheRequest,
    pub(crate) element_hwnd: IUIAutomationCacheRequest,
}

impl CacheRequests {
    pub(crate) fn build(automation: &IUIAutomation, control_view: &IUIAutomationCondition) -> Result<Self> {
        let raw_view = unsafe { automation.RawViewCondition() }.context("creating the raw view condition")?;
        let onscreen_view = onscreen_controls(automation, control_view)?;
        let name = [UIA_NamePropertyId];
        let handle = [UIA_NativeWindowHandlePropertyId];
        Ok(Self {
            subtree_props: live(automation, &ELEMENT_PROPERTIES, TreeScope_Subtree, control_view)?,
            onscreen_subtree_props: live(automation, &ELEMENT_PROPERTIES, TreeScope_Subtree, &onscreen_view)?,
            element_props: live(automation, &ELEMENT_PROPERTIES, TreeScope_Element, &raw_view)?,
            subtree_names: detached(automation, &name, TreeScope_Subtree, control_view)?,
            element_name: detached(automation, &name, TreeScope_Element, &raw_view)?,
            element_hwnd: live(automation, &handle, TreeScope_Element, &raw_view)?,
        })
    }

    pub(crate) fn subtree_props_in(&self, view: View) -> &IUIAutomationCacheRequest {
        match view {
            View::Onscreen => &self.onscreen_subtree_props,
            View::Full => &self.subtree_props,
        }
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
    let request = request(automation, properties, scope, filter)?;
    set_mode(&request, AutomationElementMode_Full)?;
    Ok(request)
}

fn detached(
    automation: &IUIAutomation,
    properties: &[UIA_PROPERTY_ID],
    scope: TreeScope,
    filter: &IUIAutomationCondition,
) -> Result<IUIAutomationCacheRequest> {
    let request = request(automation, properties, scope, filter)?;
    set_mode(&request, AutomationElementMode_None)?;
    Ok(request)
}

fn request(
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
    Ok(request)
}

fn set_mode(request: &IUIAutomationCacheRequest, mode: AutomationElementMode) -> Result<()> {
    unsafe { request.SetAutomationElementMode(mode) }.context("setting a cache request's element mode")
}
