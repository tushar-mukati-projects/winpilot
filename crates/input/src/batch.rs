use winpilot_core::keys::Modifiers;

use crate::{
    event::InputEvent,
    keyboard::{modifier_presses, modifier_releases},
    layout::KeyboardLayout,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Batch {
    pub events: Vec<InputEvent>,
    pub release_on_failure: Vec<InputEvent>,
}

impl Batch {
    pub(crate) fn plain(events: Vec<InputEvent>) -> Self {
        Self { events, release_on_failure: Vec::new() }
    }
}

pub(crate) fn with_modifiers_held(modifiers: Modifiers, inner: Vec<InputEvent>, layout: &impl KeyboardLayout) -> Batch {
    let releases = modifier_releases(modifiers, layout);
    let mut events = modifier_presses(modifiers, layout);
    events.extend(inner);
    events.extend_from_slice(&releases);
    Batch { events, release_on_failure: releases }
}
