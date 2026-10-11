use deckhand_core::Result;
use windows::Win32::UI::Input::KeyboardAndMouse::{VIRTUAL_KEY, VK_MENU};

use crate::{
    batch::Batch,
    keyboard::{KeyTransition, key_event, unscanned_key_event},
    layout::{KeyboardLayout, SystemLayout},
    send::send_batch,
};

const UNASSIGNED_KEY: VIRTUAL_KEY = VIRTUAL_KEY(0xE8);

pub fn tap_alt() -> Result<()> {
    let layout = SystemLayout::of_foreground_window();
    send_batch(&foreground_unlock_batch(&layout))
}

pub fn foreground_unlock_batch(layout: &impl KeyboardLayout) -> Batch {
    let alt_down = key_event(VK_MENU, KeyTransition::Press, layout);
    let alt_up = key_event(VK_MENU, KeyTransition::Release, layout);
    let spacer_down = unscanned_key_event(UNASSIGNED_KEY, KeyTransition::Press);
    let spacer_up = unscanned_key_event(UNASSIGNED_KEY, KeyTransition::Release);
    Batch { events: vec![alt_down, spacer_down, spacer_up, alt_up], release_on_failure: vec![spacer_up, alt_up] }
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::Input::KeyboardAndMouse::{KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP};

    use super::*;
    use crate::{
        event::{InputEvent, KeyStroke},
        test_layout::FakeLayout,
    };

    const DOWN: KEYBD_EVENT_FLAGS = KEYBD_EVENT_FLAGS(0);

    fn key(vk: VIRTUAL_KEY, scan: u16, flags: KEYBD_EVENT_FLAGS) -> InputEvent {
        InputEvent::Key(KeyStroke { vk, scan, flags })
    }

    #[test]
    fn alt_is_released_after_an_unassigned_key_so_no_menu_opens() {
        let batch = foreground_unlock_batch(&FakeLayout);
        assert_eq!(
            batch.events,
            vec![
                key(VK_MENU, 0x38, DOWN),
                key(UNASSIGNED_KEY, 0, DOWN),
                key(UNASSIGNED_KEY, 0, KEYEVENTF_KEYUP),
                key(VK_MENU, 0x38, KEYEVENTF_KEYUP),
            ]
        );
    }

    #[test]
    fn a_partial_send_releases_both_keys() {
        let batch = foreground_unlock_batch(&FakeLayout);
        assert_eq!(
            batch.release_on_failure,
            vec![key(UNASSIGNED_KEY, 0, KEYEVENTF_KEYUP), key(VK_MENU, 0x38, KEYEVENTF_KEYUP)]
        );
    }

    #[test]
    fn every_key_that_goes_down_comes_back_up() {
        let batch = foreground_unlock_batch(&FakeLayout);
        let downs = batch.events.iter().filter(|event| matches!(event, InputEvent::Key(s) if s.flags == DOWN)).count();
        assert_eq!(downs * 2, batch.events.len());
    }
}
