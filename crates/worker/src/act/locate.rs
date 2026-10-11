use deckhand_core::{Element, ElementRef, Error, ErrorCode, Point, Result, WindowInfo};

use super::{foreground::bring_forward, settle, target::Prepared};
use crate::{call_scope::CallScope, engine::Engine, on_screen::clickable_point, resolve::Reach};

#[derive(Debug, Clone)]
pub struct Located {
    pub window: WindowInfo,
    pub element: Option<Element>,
    pub point: Point,
}

pub fn locate_in_front(engine: &Engine, prepared: &Prepared, call: &CallScope) -> Result<Located> {
    let window = bring_forward(&prepared.window)?;
    locate(engine, prepared, window, call)
}

pub fn locate(engine: &Engine, prepared: &Prepared, window: WindowInfo, call: &CallScope) -> Result<Located> {
    match &prepared.reach {
        Reach::Element(element) => locate_element(engine, element.id, window, call),
        Reach::Point { point, under } => Ok(Located { window, element: Some(under.clone()), point: *point }),
        Reach::Window => Ok(Located { point: window.rect.center(), window, element: None }),
    }
}

pub fn refreshed(engine: &Engine, id: ElementRef) -> Result<Element> {
    engine.uia.run(move |uia| uia.element(id))
}

fn locate_element(engine: &Engine, id: ElementRef, window: WindowInfo, call: &CallScope) -> Result<Located> {
    let element = refreshed(engine, id)?;
    if let Some(point) = clickable_point(&element, &window) {
        return Ok(Located { window, element: Some(element), point });
    }
    if engine.uia.run(move |uia| uia.scroll_into_view(id)).is_ok() {
        settle(engine, &window, call);
    }
    let element = refreshed(engine, id)?;
    let point = clickable_point(&element, &window).ok_or_else(|| not_visible(id))?;
    Ok(Located { window, element: Some(element), point })
}

fn not_visible(id: ElementRef) -> Error {
    Error::new(ErrorCode::Obscured, format!("{id} is not visible on screen"))
}
