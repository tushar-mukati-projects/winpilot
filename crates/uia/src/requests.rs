use deckhand_core::{Context, Result};
use windows::Win32::UI::Accessibility::{
    AutomationElementMode_Full, IUIAutomation, IUIAutomationCacheRequest, IUIAutomationCondition, TreeScope,
    TreeScope_Children, TreeScope_Element, TreeScope_Subtree, UIA_ControlTypePropertyId, UIA_IsOffscreenPropertyId,
    UIA_IsSelectionItemPatternAvailablePropertyId, UIA_NamePropertyId, UIA_NativeWindowHandlePropertyId,
    UIA_PROPERTY_ID, UIA_RuntimeIdPropertyId,
};

use crate::{cached_tree::TreeRequests, properties::ELEMENT_PROPERTIES, tree_path::View, variant::OwnedVariant};

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
    subtree_props: IUIAutomationCacheRequest,
    children_props: IUIAutomationCacheRequest,
    onscreen_subtree_props: IUIAutomationCacheRequest,
    onscreen_children_props: IUIAutomationCacheRequest,
    subtree_names: IUIAutomationCacheRequest,
    children_names: IUIAutomationCacheRequest,
    subtree_options: IUIAutomationCacheRequest,
    children_options: IUIAutomationCacheRequest,
    pub(crate) element_props: IUIAutomationCacheRequest,
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
            subtree_props: live(automation, &ELEMENT_PROPERTIES, TreeScope_Subtree, control_view)?,
            children_props: live(automation, &ELEMENT_PROPERTIES, ELEMENT_AND_CHILDREN, control_view)?,
            onscreen_subtree_props: live(automation, &ELEMENT_PROPERTIES, TreeScope_Subtree, &onscreen_view)?,
            onscreen_children_props: live(automation, &ELEMENT_PROPERTIES, ELEMENT_AND_CHILDREN, &onscreen_view)?,
            subtree_names: live(automation, &NAME_PROPERTIES, TreeScope_Subtree, control_view)?,
            children_names: live(automation, &NAME_PROPERTIES, ELEMENT_AND_CHILDREN, control_view)?,
            subtree_options: live(automation, &OPTION_PROPERTIES, TreeScope_Subtree, control_view)?,
            children_options: live(automation, &OPTION_PROPERTIES, ELEMENT_AND_CHILDREN, control_view)?,
            element_props: live(automation, &ELEMENT_PROPERTIES, TreeScope_Element, &raw_view)?,
            element_hwnd: live(automation, &handle, TreeScope_Element, &raw_view)?,
            children_ids: live(automation, &runtime_id, ELEMENT_AND_CHILDREN, control_view)?,
        })
    }

    pub(crate) fn props_in(&self, view: View) -> TreeRequests<'_> {
        match view {
            View::Onscreen => {
                TreeRequests { whole: &self.onscreen_subtree_props, level: &self.onscreen_children_props }
            }
            View::Full => TreeRequests { whole: &self.subtree_props, level: &self.children_props },
        }
    }

    pub(crate) fn names(&self) -> TreeRequests<'_> {
        TreeRequests { whole: &self.subtree_names, level: &self.children_names }
    }

    pub(crate) fn options(&self) -> TreeRequests<'_> {
        TreeRequests { whole: &self.subtree_options, level: &self.children_options }
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
