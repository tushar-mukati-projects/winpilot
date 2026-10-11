use deckhand_core::{Result, TargetSpec};

use super::{done::Done, settle, target::prepare};
use crate::{call_scope::CallScope, engine::Engine};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expansion {
    Expand,
    Collapse,
}

impl Expansion {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Expand => "expand",
            Self::Collapse => "collapse",
        }
    }
}

pub fn step(engine: &Engine, target: &TargetSpec, expansion: Expansion, call: &CallScope) -> Result<Done> {
    let prepared = prepare(engine, target, call)?;
    let id = prepared.require_element(expansion.name())?.id;
    engine.uia.run(move |uia| match expansion {
        Expansion::Expand => uia.expand(id),
        Expansion::Collapse => uia.collapse(id),
    })?;
    settle(engine, &prepared.window, call);
    Ok(Done::by(expansion.name()))
}
