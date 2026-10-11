use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{KillTimer, SetTimer};
use windows::core::{Error, Result};

use crate::child::{conceal, item, reveal, set_text};
use crate::ids::{APPEAR_DELAY_MS, APPEAR_TIMER, APPEARED_BUTTON, APPEARED_STATUS};
use crate::text::APPEARED_CLICKED;

pub fn schedule(window: HWND) -> Result<()> {
    conceal(item(window, APPEARED_BUTTON)?);
    set_text(window, APPEARED_STATUS, "")?;
    if unsafe { SetTimer(Some(window), APPEAR_TIMER, APPEAR_DELAY_MS, None) } == 0 {
        return Err(Error::from_thread());
    }
    Ok(())
}

pub fn on_timer(window: HWND, timer: usize) -> Result<bool> {
    if timer != APPEAR_TIMER {
        return Ok(false);
    }
    unsafe { KillTimer(Some(window), APPEAR_TIMER)? };
    reveal(item(window, APPEARED_BUTTON)?);
    Ok(true)
}

pub fn acknowledge(window: HWND) -> Result<()> {
    set_text(window, APPEARED_STATUS, APPEARED_CLICKED)
}
