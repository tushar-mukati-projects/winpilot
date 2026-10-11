use deckhand_core::{
    Error, Result, TargetSpec,
    protocol::{Direction, MouseAction, MouseButton, Step},
};

use super::{
    buttons::{self, Press},
    done::Done,
    mode::{Mode, for_window},
    perform::required,
    pointer::{self, Drag},
    scroll::{self, ScrollRequest},
    target::{Prepared, prepare},
};
use crate::{call_scope::CallScope, engine::Engine};

pub struct MouseRequest<'a> {
    pub action: MouseAction,
    pub to: Option<&'a TargetSpec>,
    pub button: Option<MouseButton>,
    pub direction: Option<Direction>,
    pub amount: Option<u32>,
    pub glide_ms: Option<u64>,
}

pub fn step(
    engine: &Engine,
    request: &MouseRequest<'_>,
    target: Option<&Prepared>,
    step: &Step,
    mode: Mode,
    call: &CallScope,
) -> Result<Done> {
    let press = Press { button: request.button.unwrap_or_default(), glide_ms: request.glide_ms };
    match request.action {
        MouseAction::Move => pointer::move_to(engine, required(target, step)?, request.glide_ms, call),
        MouseAction::Down => buttons::mouse_down(engine, target, press, call),
        MouseAction::Up => buttons::mouse_up(engine, target, press, call),
        MouseAction::Scroll => {
            let direction =
                request.direction.ok_or_else(|| Error::invalid("mouse action=scroll needs \"direction\""))?;
            let scroll = ScrollRequest { direction, amount: request.amount, glide_ms: request.glide_ms };
            scroll::step(engine, target, &scroll, mode, call)
        }
        MouseAction::Drag => drag(engine, request, required(target, step)?, step, call),
    }
}

fn drag(engine: &Engine, request: &MouseRequest<'_>, from: &Prepared, step: &Step, call: &CallScope) -> Result<Done> {
    let to = request.to.ok_or_else(|| Error::invalid("mouse action=drag needs \"to\""))?;
    let to = prepare(engine, to, Mode::Foreground, call)?;
    for_window(Mode::Foreground, &to.window, step)?;
    let button = request.button.unwrap_or_default();
    pointer::drag(engine, &Drag { from, to: &to, button, glide_ms: request.glide_ms }, call)
}
