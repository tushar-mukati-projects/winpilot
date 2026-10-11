use deckhand_core::{Context, Result};
use windows::Win32::UI::Accessibility::{
    AutomationElementMode, AutomationElementMode_Full, AutomationElementMode_None, IUIAutomation,
    IUIAutomationCacheRequest, IUIAutomationCondition, TreeScope, TreeScope_Element, TreeScope_Subtree,
    UIA_NamePropertyId, UIA_NativeWindowHandlePropertyId, UIA_PROPERTY_ID,
};

use crate::properties::ELEMENT_PROPERTIES;

pub(crate) struct CacheRequests {
    pub(crate) subtree: IUIAutomationCacheRequest,
    pub(crate) single: IUIAutomationCacheRequest,
    pub(crate) names: IUIAutomationCacheRequest,
    pub(crate) name_only: IUIAutomationCacheRequest,
    pub(crate) native_window: IUIAutomationCacheRequest,
}

impl CacheRequests {
    pub(crate) fn build(automation: &IUIAutomation, control_view: &IUIAutomationCondition) -> Result<Self> {
        let raw_view = unsafe { automation.RawViewCondition() }.context("creating the raw view condition")?;
        let name = [UIA_NamePropertyId];
        let handle = [UIA_NativeWindowHandlePropertyId];
        Ok(Self {
            subtree: live(automation, &ELEMENT_PROPERTIES, TreeScope_Subtree, control_view)?,
            single: live(automation, &ELEMENT_PROPERTIES, TreeScope_Element, &raw_view)?,
            names: detached(automation, &name, TreeScope_Subtree, control_view)?,
            name_only: detached(automation, &name, TreeScope_Element, &raw_view)?,
            native_window: live(automation, &handle, TreeScope_Element, &raw_view)?,
        })
    }
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
