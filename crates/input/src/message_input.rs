use std::time::Duration;

use deckhand_core::{
    Point, Result, WindowRef,
    keys::Chord,
    protocol::{Direction, MouseButton},
};
use windows::Win32::UI::Input::KeyboardAndMouse::VK_RETURN;

use crate::{
    layout::SystemLayout,
    message_delivery::{deliver, deliver_releasing},
    message_plan::{click_plan, drag_plan, key_code, keys_plan, release_plan, scroll_plan, text_plan},
    message_spot::{MessageSpot, client_point, live_hwnd},
    mouse::validate_click_count,
};

pub fn message_click(
    spot: &MessageSpot,
    button: MouseButton,
    count: u8,
    check: impl FnMut() -> Result<()>,
) -> Result<()> {
    validate_click_count(count)?;
    let hwnd = live_hwnd(spot.window)?;
    deliver_releasing(hwnd, &click_plan(spot.client, button, count), release_plan(spot.client, button), check)
}

pub fn message_drag(
    from: &MessageSpot,
    to: Point,
    button: MouseButton,
    travel: Duration,
    check: impl FnMut() -> Result<()>,
) -> Result<()> {
    let hwnd = live_hwnd(from.window)?;
    let to = client_point(hwnd, to)?;
    deliver_releasing(hwnd, &drag_plan(from.client, to, button, travel), release_plan(from.client, button), check)
}

pub fn message_scroll(
    spot: &MessageSpot,
    direction: Direction,
    notches: u32,
    check: impl FnMut() -> Result<()>,
) -> Result<()> {
    let hwnd = live_hwnd(spot.window)?;
    deliver(hwnd, &scroll_plan(spot.client, spot.screen, direction, notches)?, check)
}

pub fn message_text(window: WindowRef, text: &str, check: impl FnMut() -> Result<()>) -> Result<()> {
    let hwnd = live_hwnd(window)?;
    let enter = key_code(VK_RETURN, &SystemLayout::of_window(hwnd));
    deliver(hwnd, &text_plan(text, enter), check)
}

pub fn message_keys(
    window: WindowRef,
    chords: &[Chord],
    repeat: u32,
    mut check: impl FnMut() -> Result<()>,
) -> Result<()> {
    let hwnd = live_hwnd(window)?;
    let plan = keys_plan(chords, &SystemLayout::of_window(hwnd))?;
    for _ in 0..repeat {
        deliver(hwnd, &plan, &mut check)?;
    }
    Ok(())
}
