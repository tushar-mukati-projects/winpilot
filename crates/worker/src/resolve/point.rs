use winpilot_core::{ImageRef, Point, Result, WindowInfo};

use super::{Cover, Reach, Resolved, element_window};
use crate::{cover, engine::Engine, images::ImageRecord};

pub(super) fn resolve_point(
    engine: &Engine,
    point: Point,
    image: Option<ImageRef>,
    covering: Cover,
) -> Result<Resolved> {
    let record = image.map(|image| engine.images.lock().record(image).map(|record| (image, record))).transpose()?;
    let screen = match &record {
        Some((image, record)) => record.screen_point(*image, point)?,
        None => point,
    };
    let under = engine.uia.run(move |uia| uia.element_at(screen))?;
    let hit = element_window(engine, under.id)?;
    let window = match &record {
        Some((_, record)) => shot_window(engine, record, &hit, covering)?,
        None => hit.clone(),
    };
    engine.guard.check_window(&hit)?;
    engine.guard.check_window(&window)?;
    Ok(Resolved { window, reach: Reach::Point { point: screen, under } })
}

fn shot_window(engine: &Engine, record: &ImageRecord, hit: &WindowInfo, covering: Cover) -> Result<WindowInfo> {
    if covering == Cover::Refused {
        record.check_hit_window(hit.hwnd, |source| cover::window_covers(engine, source, hit.hwnd))?;
    }
    match record.window.filter(|source| *source != hit.hwnd) {
        Some(source) => winpilot_desktop::window(source),
        None => Ok(hit.clone()),
    }
}
