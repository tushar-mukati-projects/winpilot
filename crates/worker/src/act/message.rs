use deckhand_core::{
    Error, ErrorCode, Point, Result, WindowInfo,
    protocol::{Direction, MouseButton},
};
use deckhand_input::MessageSpot;

use super::{
    by_pattern::by_pattern, click_choice::ClickOptions, done::Done, locate::locate_in_place, motion::Motion,
    pointer::SHORTEST_DRAG, target::Prepared,
};
use crate::{call_scope::CallScope, engine::Engine, host_terminal, overlay};

pub(crate) const MESSAGE: &str = "message";

pub(crate) struct Spot {
    pub(crate) window: WindowInfo,
    pub(crate) point: Point,
    pub(crate) target: MessageSpot,
}

pub(crate) struct MessageDrag<'a> {
    pub(crate) from: &'a Prepared,
    pub(crate) to: &'a Prepared,
    pub(crate) button: MouseButton,
    pub(crate) glide_ms: Option<u64>,
}

pub(crate) fn spot(engine: &Engine, prepared: &Prepared, call: &CallScope) -> Result<Spot> {
    let located = locate_in_place(engine, prepared, call)?;
    spot_at(engine, &located.window, located.point, call)
}

pub(crate) fn click(engine: &Engine, prepared: &Prepared, options: ClickOptions, call: &CallScope) -> Result<Done> {
    prepared.require_element("a background click")?;
    let spot = spot(engine, prepared, call)?;
    let note = click_spot(engine, &spot, options.button, options.count, call)?;
    Ok(Done::by(MESSAGE).with_note(note))
}

pub(crate) fn click_spot(
    engine: &Engine,
    spot: &Spot,
    button: MouseButton,
    count: u8,
    call: &CallScope,
) -> Result<Option<String>> {
    let check = || engine.guard.check_running(call);
    let ((), note) =
        by_pattern(engine, &spot.window, call, || deckhand_input::message_click(&spot.target, button, count, check))?;
    show_click(engine, spot, count);
    Ok(note)
}

pub(crate) fn scroll(
    engine: &Engine,
    prepared: &Prepared,
    direction: Direction,
    notches: u32,
    call: &CallScope,
) -> Result<Done> {
    let spot = spot(engine, prepared, call)?;
    let check = || engine.guard.check_running(call);
    let ((), note) = by_pattern(engine, &spot.window, call, || {
        deckhand_input::message_scroll(&spot.target, direction, notches, check)
    })?;
    show_click(engine, &spot, 1);
    Ok(Done::by(MESSAGE).with_note(note))
}

pub(crate) fn drag(engine: &Engine, drag: &MessageDrag<'_>, call: &CallScope) -> Result<Done> {
    drag.from.require_element("a background drag")?;
    let grab = spot(engine, drag.from, call)?;
    let release = locate_in_place(engine, drag.to, call)?;
    if release.window.hwnd != grab.window.hwnd {
        return Err(across_windows(&grab.window, &release.window));
    }
    require_inside(&release.window, release.point)?;
    host_terminal::check_point(engine, &release.window, release.point)?;
    let travel = Motion::start(engine, call).glide_duration(drag.glide_ms).max(SHORTEST_DRAG);
    let check = || engine.guard.check_running(call);
    let ((), note) = by_pattern(engine, &grab.window, call, || {
        deckhand_input::message_drag(&grab.target, release.point, drag.button, travel, check)
    })?;
    if grab.window.on_current_desktop {
        overlay::glide(engine, grab.point, release.point, travel);
    }
    Ok(Done::by(MESSAGE).with_note(note))
}

fn spot_at(engine: &Engine, window: &WindowInfo, point: Point, call: &CallScope) -> Result<Spot> {
    engine.guard.before_patterns(Some(window), call)?;
    require_not_in_use(window)?;
    require_inside(window, point)?;
    host_terminal::check_point(engine, window, point)?;
    let target = deckhand_input::message_spot(window.hwnd, point)?;
    if !same_app(window.pid, target.pid, deckhand_desktop::process_image_path) {
        return Err(other_process(window, point));
    }
    Ok(Spot { window: window.clone(), point, target })
}

fn show_click(engine: &Engine, spot: &Spot, count: u8) {
    if spot.window.on_current_desktop {
        overlay::click(engine, spot.point, count);
    }
}

pub(crate) fn require_not_in_use(window: &WindowInfo) -> Result<()> {
    if !window.foreground {
        return Ok(());
    }
    Err(Error::new(
        ErrorCode::NeedsUser,
        format!(
            "{} ({}) is the window the user is working in, so background mode sends it no window messages; target the app's own window (a ref, or a point with the img of its window screenshot) or drop background",
            window.hwnd, window.process
        ),
    ))
}

fn require_inside(window: &WindowInfo, point: Point) -> Result<()> {
    if window.rect.contains(point) {
        return Ok(());
    }
    Err(Error::new(
        ErrorCode::Obscured,
        format!("{},{} is outside {}, so no window message can reach it there", point.x, point.y, window.hwnd),
    ))
}

fn same_app(window_pid: u32, spot_pid: u32, image_path: impl Fn(u32) -> Option<String>) -> bool {
    if window_pid == spot_pid {
        return true;
    }
    match (image_path(window_pid), image_path(spot_pid)) {
        (Some(window), Some(spot)) => window.eq_ignore_ascii_case(&spot),
        _ => false,
    }
}

fn other_process(window: &WindowInfo, point: Point) -> Error {
    Error::new(
        ErrorCode::Denied,
        format!(
            "{},{} in {} belongs to a window of another process, and Deckhand only sends window messages to the target app",
            point.x, point.y, window.hwnd
        ),
    )
}

fn across_windows(from: &WindowInfo, to: &WindowInfo) -> Error {
    Error::invalid(format!(
        "a background drag stays inside one window, but it starts in {} and ends in {}; drop background to drag between windows",
        from.hwnd, to.hwnd
    ))
}

#[cfg(test)]
mod tests {
    use deckhand_core::Rect;

    use super::*;
    use crate::test_fixture::window;

    fn paths(pid: u32) -> Option<String> {
        match pid {
            1 | 2 => Some("C:\\Apps\\CapCut.exe".to_string()),
            3 => Some("c:\\apps\\capcut.EXE".to_string()),
            4 => Some("C:\\Other\\Helper.exe".to_string()),
            _ => None,
        }
    }

    #[test]
    fn the_same_process_or_the_same_program_may_get_messages() {
        assert!(same_app(7, 7, |_| panic!("the path is not needed for the same process")));
        assert!(same_app(1, 2, paths));
        assert!(same_app(1, 3, paths));
        assert!(!same_app(1, 4, paths));
        assert!(!same_app(1, 9, paths));
        assert!(!same_app(9, 10, paths));
    }

    #[test]
    fn refusals_name_the_spot_and_the_window() {
        let app = window(5, "capcut.exe", "CapCut", Rect::new(0, 0, 100, 100));
        let other = other_process(&app, Point::new(10, 20));
        assert_eq!(other.code, ErrorCode::Denied);
        assert!(other.message.starts_with("10,20 in w5 belongs to a window of another process"), "{}", other.message);
        let outside = require_inside(&app, Point::new(150, 20)).unwrap_err();
        assert_eq!(outside.code, ErrorCode::Obscured);
        assert!(require_inside(&app, Point::new(50, 50)).is_ok());
        let elsewhere = window(6, "capcut.exe", "Export", Rect::new(0, 0, 10, 10));
        let across = across_windows(&app, &elsewhere);
        assert!(across.message.contains("starts in w5 and ends in w6"), "{}", across.message);
    }

    #[test]
    fn the_window_the_user_is_in_gets_no_messages() {
        let behind = window(5, "capcut.exe", "CapCut", Rect::new(0, 0, 100, 100));
        assert!(require_not_in_use(&behind).is_ok());
        let in_use = WindowInfo { foreground: true, ..window(7, "chrome.exe", "Video", Rect::new(0, 0, 100, 100)) };
        let refused = require_not_in_use(&in_use).unwrap_err();
        assert_eq!(refused.code, ErrorCode::NeedsUser);
        assert!(
            refused.message.starts_with("w7 (chrome.exe) is the window the user is working in"),
            "{}",
            refused.message
        );
    }
}
