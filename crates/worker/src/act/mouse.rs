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

pub(crate) struct MouseRequest<'a> {
    pub(crate) action: MouseAction,
    pub(crate) to: Option<&'a TargetSpec>,
    pub(crate) button: Option<MouseButton>,
    pub(crate) direction: Option<Direction>,
    pub(crate) amount: Option<u32>,
    pub(crate) glide_ms: Option<u64>,
}

impl<'a> TryFrom<&'a Step> for MouseRequest<'a> {
    type Error = Error;

    fn try_from(step: &'a Step) -> Result<Self> {
        let Step::Mouse { action, to, button, direction, amount, glide_ms, .. } = step else {
            return Err(Error::invalid(format!("{} is not a mouse step", step.kind())));
        };
        Ok(Self {
            action: *action,
            to: to.as_ref(),
            button: *button,
            direction: *direction,
            amount: *amount,
            glide_ms: *glide_ms,
        })
    }
}

pub(crate) fn step(
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

#[cfg(test)]
mod tests {
    use super::*;

    fn step(json: &str) -> Step {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn only_mouse_steps_become_mouse_requests() {
        let drag = step(r#"{"do":"mouse","action":"drag","target":"e1","to":"e2","glide_ms":50}"#);
        let request = MouseRequest::try_from(&drag).unwrap();
        assert_eq!((request.action, request.glide_ms), (MouseAction::Drag, Some(50)));
        assert_eq!(request.to, Some(&TargetSpec::Text("e2".to_string())));
        assert!(MouseRequest::try_from(&step(r#"{"do":"wait","ms":5}"#)).is_err());
    }
}
