use std::cell::Cell;

use windows::Win32::Foundation::HWND;

use crate::font::GuiFont;

thread_local! {
    static CLICKS: Cell<u32> = const { Cell::new(0) };
    static REMEMBERED_FOCUS: Cell<Option<HWND>> = const { Cell::new(None) };
    static FONT: Cell<Option<GuiFont>> = const { Cell::new(None) };
}

pub struct StateGuard;

impl StateGuard {
    pub fn new() -> Self {
        reset();
        Self
    }
}

impl Drop for StateGuard {
    fn drop(&mut self) {
        reset();
    }
}

fn reset() {
    CLICKS.set(0);
    REMEMBERED_FOCUS.set(None);
    drop(FONT.take());
}

pub fn record_click() -> u32 {
    let count = CLICKS.get().saturating_add(1);
    CLICKS.set(count);
    count
}

pub fn remember_focus(control: HWND) {
    REMEMBERED_FOCUS.set(Some(control));
}

pub fn remembered_focus() -> Option<HWND> {
    REMEMBERED_FOCUS.get()
}

pub fn adopt_font(font: GuiFont) {
    drop(FONT.replace(Some(font)));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clicks_count_up_from_one_and_reset_with_the_guard() {
        let guard = StateGuard::new();
        assert_eq!(record_click(), 1);
        assert_eq!(record_click(), 2);
        drop(guard);
        let _guard = StateGuard::new();
        assert_eq!(record_click(), 1);
    }

    #[test]
    fn click_count_saturates() {
        let _guard = StateGuard::new();
        CLICKS.set(u32::MAX);
        assert_eq!(record_click(), u32::MAX);
    }

    #[test]
    fn focus_is_forgotten_on_reset() {
        let guard = StateGuard::new();
        remember_focus(HWND(std::ptr::without_provenance_mut(0x1234)));
        assert!(remembered_focus().is_some());
        drop(guard);
        assert!(remembered_focus().is_none());
    }
}
