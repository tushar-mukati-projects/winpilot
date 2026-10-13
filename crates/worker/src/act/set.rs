use winpilot_core::{Result, protocol::SetChange};

use super::{
    done::Done,
    expand::{self, Expansion},
    mode::Mode,
    select,
    target::Prepared,
    toggle, value,
};
use crate::{call_scope::CallScope, engine::Engine};

pub(crate) fn step(
    engine: &Engine,
    prepared: &Prepared,
    change: &SetChange,
    mode: Mode,
    call: &CallScope,
) -> Result<Done> {
    match change {
        SetChange::Value(text) => value::step(engine, prepared, text, mode, call),
        SetChange::Option(option) => select::option(engine, prepared, option, mode, call),
        SetChange::Checked(checked) => toggle::step(engine, prepared, *checked, mode, call),
        SetChange::Expanded(expanded) => expand::step(engine, prepared, Expansion::wanted(*expanded), call),
        SetChange::Selected(selected) => select::item(engine, prepared, *selected, call),
    }
}
