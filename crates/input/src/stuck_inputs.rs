use deckhand_core::Result;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VIRTUAL_KEY, VK_LBUTTON, VK_LCONTROL, VK_LMENU, VK_LSHIFT, VK_LWIN, VK_MBUTTON, VK_RBUTTON,
    VK_RCONTROL, VK_RMENU, VK_RSHIFT, VK_RWIN,
};

use crate::{
    event::InputEvent,
    keyboard::{KeyTransition, key_event},
    layout::{KeyboardLayout, SystemLayout},
    mouse::{ButtonTransition, button_event},
    physical_button::PhysicalButton,
    send::send,
};

const MOUSE_BUTTONS: [(VIRTUAL_KEY, PhysicalButton); 3] =
    [(VK_LBUTTON, PhysicalButton::Left), (VK_RBUTTON, PhysicalButton::Right), (VK_MBUTTON, PhysicalButton::Middle)];
const MODIFIER_KEYS: [VIRTUAL_KEY; 8] =
    [VK_LCONTROL, VK_RCONTROL, VK_LMENU, VK_RMENU, VK_LSHIFT, VK_RSHIFT, VK_LWIN, VK_RWIN];

pub fn release_stuck_inputs() -> Result<()> {
    let layout = SystemLayout::of_foreground_window();
    let releases = stuck_input_releases(is_logically_down, &layout);
    if releases.is_empty() {
        return Ok(());
    }
    send(&releases)
}

fn stuck_input_releases(is_down: impl Fn(VIRTUAL_KEY) -> bool, layout: &impl KeyboardLayout) -> Vec<InputEvent> {
    let buttons = MOUSE_BUTTONS
        .into_iter()
        .filter(|(vk, _)| is_down(*vk))
        .map(|(_, button)| button_event(button, ButtonTransition::Release));
    let modifiers =
        MODIFIER_KEYS.into_iter().filter(|vk| is_down(*vk)).map(|vk| key_event(vk, KeyTransition::Release, layout));
    buttons.chain(modifiers).collect()
}

fn is_logically_down(vk: VIRTUAL_KEY) -> bool {
    let state = unsafe { GetAsyncKeyState(i32::from(vk.0)) };
    state < 0
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_RIGHTUP,
    };

    use super::*;
    use crate::{event::KeyStroke, test_layout::FakeLayout};

    fn releases_for(down: &[VIRTUAL_KEY]) -> Vec<InputEvent> {
        stuck_input_releases(|vk| down.contains(&vk), &FakeLayout)
    }

    fn key_up(event: &InputEvent) -> Option<KeyStroke> {
        match event {
            InputEvent::Key(stroke) if stroke.flags.contains(KEYEVENTF_KEYUP) => Some(*stroke),
            _ => None,
        }
    }

    #[test]
    fn nothing_down_means_nothing_to_release() {
        assert!(releases_for(&[]).is_empty());
    }

    #[test]
    fn held_buttons_are_released_before_held_modifiers() {
        let events = releases_for(&[VK_RSHIFT, VK_LBUTTON, VK_RBUTTON]);
        assert_eq!(events.len(), 3);
        assert!(matches!(events[0], InputEvent::Mouse(action) if action.flags == MOUSEEVENTF_LEFTUP));
        assert!(matches!(events[1], InputEvent::Mouse(action) if action.flags == MOUSEEVENTF_RIGHTUP));
        assert_eq!(key_up(&events[2]).map(|stroke| stroke.vk), Some(VK_RSHIFT));
    }

    #[test]
    fn every_modifier_side_is_released_as_a_key_up() {
        let events = releases_for(&MODIFIER_KEYS);
        let released: Vec<VIRTUAL_KEY> = events.iter().filter_map(key_up).map(|stroke| stroke.vk).collect();
        assert_eq!(released, MODIFIER_KEYS);
    }

    #[test]
    fn right_hand_ctrl_alt_and_both_win_keys_are_extended() {
        let events = releases_for(&[VK_LCONTROL, VK_RCONTROL, VK_RMENU, VK_LWIN]);
        let extended: Vec<bool> =
            events.iter().filter_map(key_up).map(|stroke| stroke.flags.contains(KEYEVENTF_EXTENDEDKEY)).collect();
        assert_eq!(extended, vec![false, true, true, true]);
    }

    #[test]
    fn reading_key_state_does_not_panic() {
        let _down = is_logically_down(VK_LCONTROL);
    }
}
