use deckhand_core::{Element, Error, ErrorCode, Point, Result, WindowInfo, protocol::Direction};

use super::{
    by_pattern::by_pattern,
    done::Done,
    hit::lands_in,
    locate::refreshed,
    mode::Mode,
    motion::Motion,
    settle,
    target::{Prepared, given_or_else, prepare_default_window},
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
    let prepared = given_or_else(target, || prepare_default_window(engine, call))?;
    let wheel_at = match mode {
        Mode::Foreground => wheel_point(engine, &prepared)?,
        Mode::Background => None,
    };
    match (wheel_at, prepared.element_or_point_hit()) {
        (Some(point), _) => wheel(engine, &prepared.window, point, request, call),
        (None, Some(element)) => scroll_by_pattern(engine, &prepared.window, element, request, call),
        (None, None) => Err(no_scroll_target(&prepared.window, mode)),
    }
}

pub fn edge_note(moved: u32) -> Option<String> {
    (moved == 0).then(|| AT_EDGE.to_string())
}

fn wheel(
    engine: &Engine,
    window: &WindowInfo,
    point: Point,
    request: &ScrollRequest,
    call: &CallScope,
) -> Result<Done> {
    let motion = Motion::start(engine, call);
    motion.glide_to(point, request.glide_ms)?;
    if !lands_in(engine, point, window) {
        return Err(no_scroll_target(window, Mode::Foreground));
    }
    motion.check()?;
    deckhand_input::scroll(point, request.direction, amount(request))?;
    overlay::click(engine, point, 1);
    settle(engine, window, call);
    Ok(Done::by("wheel"))
}

fn scroll_by_pattern(
    engine: &Engine,
    window: &WindowInfo,
    element: &Element,
    request: &ScrollRequest,
    call: &CallScope,
) -> Result<Done> {
    let (id, direction, amount) = (element.id, request.direction, amount(request));
    let (moved, note) =
        by_pattern(engine, window, call, || engine.uia.run(move |uia| uia.scroll(id, direction, amount)))?;
    Ok(Done::by("pattern").with_note(edge_note(moved)).with_note(note))
}

fn amount(request: &ScrollRequest) -> u32 {
    request.amount.unwrap_or(DEFAULT_AMOUNT)
}

fn wheel_point(engine: &Engine, prepared: &Prepared) -> Result<Option<Point>> {
    let point = current_point(engine, prepared)?;
    Ok(point.filter(|point| lands_in(engine, *point, &prepared.window)))
}

fn current_point(engine: &Engine, prepared: &Prepared) -> Result<Option<Point>> {
    let window = deckhand_desktop::window(prepared.window.hwnd)?;
    Ok(match &prepared.reach {
        Reach::Element(element) => clickable_point(&refreshed(engine, element.id)?, &window),
        Reach::Point { point, .. } => Some(*point),
        Reach::Window => Some(window.rect.center()),
    })
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

    #[test]
    fn the_default_amount_is_three_notches() {
        let request = ScrollRequest { direction: Direction::Down, amount: None, glide_ms: None };
        assert_eq!(amount(&request), DEFAULT_AMOUNT);
        assert_eq!(amount(&ScrollRequest { amount: Some(7), ..request }), 7);
    }
}
