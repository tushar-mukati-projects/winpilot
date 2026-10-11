use deckhand_core::{Error, ErrorCode, Point, Result, WindowInfo, protocol::Direction};

use super::{
    done::Done,
    locate::refreshed,
    mode::Mode,
    motion::move_pointer,
    settle,
    target::{Prepared, prepare_default_window},
};
use crate::{call_scope::CallScope, engine::Engine, on_screen::clickable_point, overlay, resolve::Reach};

const AT_EDGE: &str = "already at the edge";
const DEFAULT_AMOUNT: u32 = 3;

pub struct ScrollRequest {
    pub direction: Direction,
    pub amount: Option<u32>,
    pub glide_ms: Option<u64>,
}

pub fn step(
    engine: &Engine,
    target: Option<&Prepared>,
    request: &ScrollRequest,
    mode: Mode,
    call: &CallScope,
) -> Result<Done> {
    let default_window;
    let prepared = if let Some(prepared) = target {
        prepared
    } else {
        default_window = prepare_default_window(engine, call)?;
        &default_window
    };
    let (direction, amount) = (request.direction, request.amount.unwrap_or(DEFAULT_AMOUNT));
    let wheel_at = match mode {
        Mode::Foreground => wheel_point(engine, prepared)?,
        Mode::Background => None,
    };
    let (method, note) = match (wheel_at, prepared.element()) {
        (Some(point), _) => {
            move_pointer(engine, point, request.glide_ms)?;
            deckhand_input::scroll(point, direction, amount)?;
            overlay::click(engine, point, 1);
            ("wheel", None)
        }
        (None, Some(element)) => {
            let id = element.id;
            let moved = engine.uia.run(move |uia| uia.scroll(id, direction, amount))?;
            ("pattern", edge_note(moved))
        }
        (None, None) => return Err(no_scroll_target(&prepared.window, mode)),
    };
    settle(engine, &prepared.window, call);
    Ok(Done::by(method).with_note(note))
}

pub fn edge_note(moved: u32) -> Option<String> {
    (moved == 0).then(|| AT_EDGE.to_string())
}

fn wheel_point(engine: &Engine, prepared: &Prepared) -> Result<Option<Point>> {
    let point = current_point(engine, prepared)?;
    Ok(point.filter(|point| wheel_lands_in(engine, *point, &prepared.window)))
}

fn current_point(engine: &Engine, prepared: &Prepared) -> Result<Option<Point>> {
    let window = deckhand_desktop::window(prepared.window.hwnd)?;
    Ok(match &prepared.reach {
        Reach::Element(element) => clickable_point(&refreshed(engine, element.id)?, &window),
        Reach::Point { point, .. } => Some(*point),
        Reach::Window => Some(window.rect.center()),
    })
}

fn wheel_lands_in(engine: &Engine, point: Point, window: &WindowInfo) -> bool {
    let landed = engine.uia.run(move |uia| {
        let hit = uia.element_at(point)?;
        uia.window_of(hit.id)
    });
    landed.is_ok_and(|hwnd| hwnd == window.hwnd)
}

fn no_scroll_target(window: &WindowInfo, mode: Mode) -> Error {
    match mode {
        Mode::Foreground => Error::new(
            ErrorCode::Obscured,
            format!("another window covers {} where the wheel would scroll", window.hwnd),
        ),
        Mode::Background => Error::invalid(format!(
            "scrolling {} without the real mouse needs an element with a scroll pattern as target",
            window.hwnd
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scrolling_nowhere_notes_the_edge() {
        assert_eq!(edge_note(0).as_deref(), Some(AT_EDGE));
        assert_eq!(edge_note(3), None);
    }
}
