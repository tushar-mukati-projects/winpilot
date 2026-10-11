use deckhand_core::{Result, protocol::Step};

use super::{
    buttons::{self, Press},
    click,
    click_choice::ClickOptions,
    done::Done,
    expand::{self, Expansion},
    focus_step, keys, pause, pointer,
    scroll::{self, ScrollRequest},
    select, toggle,
    typing::{self, TypeRequest},
    value, wait_step,
};
use crate::{call_scope::CallScope, engine::Engine, wait::WindowsBefore};

pub fn perform(engine: &Engine, step: &Step, call: &CallScope, before: &WindowsBefore) -> Result<Done> {
    match step {
        Step::Click { target, button, count, modifiers, how, glide_ms } => {
            let options =
                ClickOptions { glide_ms: *glide_ms, ..ClickOptions::new(*button, *count, modifiers.as_deref(), *how)? };
            click::step(engine, target, options, call)
        }
        Step::Hover { target, glide_ms } => pointer::hover(engine, target, *glide_ms, call),
        Step::Move { to, glide_ms } => buttons::move_step(engine, to, *glide_ms, call),
        Step::MouseDown { target, button, glide_ms } => {
            buttons::mouse_down(engine, &Press { target: Some(target), button: *button, glide_ms: *glide_ms }, call)
        }
        Step::MouseUp { target, button, glide_ms } => {
            buttons::mouse_up(engine, &Press { target: target.as_ref(), button: *button, glide_ms: *glide_ms }, call)
        }
        Step::Type { target, text, enter, how } => {
            let request = TypeRequest { target: target.as_ref(), text, enter: *enter, how: *how };
            typing::step(engine, &request, call)
        }
        Step::SetValue { target, value } => value::step(engine, target, value, call),
        Step::Key { keys, target, repeat } => keys::step(engine, keys, target.as_ref(), *repeat, call),
        Step::Scroll { target, direction, amount, glide_ms } => {
            let request =
                ScrollRequest { target: target.as_ref(), direction: *direction, amount: *amount, glide_ms: *glide_ms };
            scroll::step(engine, &request, call)
        }
        Step::Drag { from, to, glide_ms } => pointer::drag(engine, (from, to), *glide_ms, call),
        Step::Select { target, option } => select::step(engine, target, option.as_deref(), call),
        Step::Toggle { target, on } => toggle::step(engine, target, *on, call),
        Step::Expand { target } => expand::step(engine, target, Expansion::Expand, call),
        Step::Collapse { target } => expand::step(engine, target, Expansion::Collapse, call),
        Step::Focus { target } => focus_step::step(engine, target, call),
        Step::WaitFor(condition) => wait_step::step(engine, condition, call, before),
        Step::Wait { ms } => pause::step(engine, *ms, call),
    }
}
