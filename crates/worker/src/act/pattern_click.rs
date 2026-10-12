use deckhand_core::{Element, ElementRef, ErrorCode, Result};

use super::click_choice::{PatternAction, pattern_actions};
use crate::engine::Engine;

pub(crate) fn click_by_pattern(engine: &Engine, element: &Element) -> Result<Option<&'static str>> {
    for action in pattern_actions(element.patterns, element.state.expanded) {
        match perform(engine, element.id, action) {
            Ok(()) => return Ok(Some(action.name())),
            Err(error) if error.code == ErrorCode::Unsupported => {}
            Err(error) => return Err(error),
        }
    }
    Ok(None)
}

fn perform(engine: &Engine, id: ElementRef, action: PatternAction) -> Result<()> {
    engine.uia.run(move |uia| match action {
        PatternAction::Invoke => uia.invoke(id),
        PatternAction::Toggle => uia.toggle(id).map(drop),
        PatternAction::Select => uia.select(id),
        PatternAction::Expand => uia.expand(id),
        PatternAction::Collapse => uia.collapse(id),
        PatternAction::DefaultAction => uia.default_action(id),
    })
}
