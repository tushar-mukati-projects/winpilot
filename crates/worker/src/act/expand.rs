use deckhand_core::Result;

use super::{by_pattern::by_pattern, done::Done, target::Prepared};
use crate::{call_scope::CallScope, engine::Engine};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expansion {
    Expand,
    Collapse,
}

impl Expansion {
    pub const fn wanted(expanded: bool) -> Self {
        if expanded { Self::Expand } else { Self::Collapse }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Expand => "expand",
            Self::Collapse => "collapse",
        }
    }
}

pub fn step(engine: &Engine, prepared: &Prepared, expansion: Expansion, call: &CallScope) -> Result<Done> {
    let id = prepared.require_element("set expanded")?.id;
    let ((), note) = by_pattern(engine, &prepared.window, call, || {
        engine.uia.run(move |uia| match expansion {
            Expansion::Expand => uia.expand(id),
            Expansion::Collapse => uia.collapse(id),
        })
    })?;
    Ok(Done::by(expansion.name()).with_note(note))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wish_picks_the_pattern_call() {
        assert_eq!(Expansion::wanted(true), Expansion::Expand);
        assert_eq!(Expansion::wanted(false).name(), "collapse");
    }
}
