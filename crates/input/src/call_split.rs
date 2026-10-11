use windows::Win32::UI::Input::KeyboardAndMouse::{
    KEYEVENTF_KEYUP, KEYEVENTF_UNICODE, MOUSE_EVENT_FLAGS, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_XUP,
};

use crate::event::{InputEvent, KeyStroke, MouseAction};

pub const MAX_INPUTS_PER_CALL: usize = 512;

const LOW_SURROGATES: std::ops::RangeInclusive<u16> = 0xDC00..=0xDFFF;
const BUTTON_RELEASES: MOUSE_EVENT_FLAGS =
    MOUSE_EVENT_FLAGS(MOUSEEVENTF_LEFTUP.0 | MOUSEEVENTF_RIGHTUP.0 | MOUSEEVENTF_MIDDLEUP.0 | MOUSEEVENTF_XUP.0);

pub fn split_into_calls(events: &[InputEvent]) -> Vec<&[InputEvent]> {
    let mut calls = Vec::with_capacity(events.len() / MAX_INPUTS_PER_CALL + 1);
    let mut rest = events;
    while !rest.is_empty() {
        let (call, remaining) = rest.split_at(call_length(rest));
        calls.push(call);
        rest = remaining;
    }
    calls
}

fn call_length(events: &[InputEvent]) -> usize {
    if events.len() <= MAX_INPUTS_PER_CALL {
        return events.len();
    }
    (1..=MAX_INPUTS_PER_CALL)
        .rev()
        .find(|&index| events.get(index).is_some_and(starts_group))
        .unwrap_or(MAX_INPUTS_PER_CALL)
}

fn starts_group(event: &InputEvent) -> bool {
    match *event {
        InputEvent::Key(stroke) => !stroke.flags.contains(KEYEVENTF_KEYUP) && !is_low_surrogate(stroke),
        InputEvent::Mouse(action) => !releases_a_button(action),
    }
}

fn is_low_surrogate(stroke: KeyStroke) -> bool {
    stroke.flags.contains(KEYEVENTF_UNICODE) && LOW_SURROGATES.contains(&stroke.scan)
}

fn releases_a_button(action: MouseAction) -> bool {
    action.flags.0 & BUTTON_RELEASES.0 != 0
}

#[cfg(test)]
mod tests {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        KEYBD_EVENT_FLAGS, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_MOVE, VIRTUAL_KEY,
    };

    use super::*;
    use crate::{test_layout::FakeLayout, text::text_events};

    fn sizes(events: &[InputEvent]) -> Vec<usize> {
        split_into_calls(events).iter().map(|call| call.len()).collect()
    }

    fn key(scan: u16, flags: KEYBD_EVENT_FLAGS) -> InputEvent {
        InputEvent::Key(KeyStroke { vk: VIRTUAL_KEY(0), scan, flags })
    }

    fn mouse(flags: MOUSE_EVENT_FLAGS) -> InputEvent {
        InputEvent::Mouse(MouseAction { dx: 0, dy: 0, data: 0, flags })
    }

    #[test]
    fn long_text_is_chunked_on_whole_characters() {
        let events = text_events(&"x".repeat(2_000), &FakeLayout);
        assert_eq!(sizes(&events), vec![512, 512, 512, 512, 512, 512, 512, 416]);
    }

    #[test]
    fn surrogate_pair_never_straddles_two_calls() {
        let text = format!("{}😀", "x".repeat(255));
        let events = text_events(&text, &FakeLayout);
        assert_eq!(events.len(), 514);
        let calls = split_into_calls(&events);
        assert_eq!(calls.iter().map(|call| call.len()).collect::<Vec<_>>(), vec![510, 4]);
        assert_eq!(calls[1], &events[510..]);
        assert!(matches!(calls[1][0], InputEvent::Key(stroke) if stroke.scan == 0xD83D));
        assert!(matches!(calls[1][3], InputEvent::Key(stroke) if stroke.scan == 0xDE00));
    }

    #[test]
    fn every_call_starts_a_character_and_respects_the_limit() {
        let text = "a😀\nb".repeat(300);
        let events = text_events(&text, &FakeLayout);
        let calls = split_into_calls(&events);
        assert!(calls.len() > 1);
        assert!(calls.iter().all(|call| call.len() <= MAX_INPUTS_PER_CALL));
        assert!(calls.iter().all(|call| starts_group(&call[0])));
        assert_eq!(calls.iter().map(|call| call.len()).sum::<usize>(), events.len());
    }

    #[test]
    fn releases_and_low_surrogates_continue_a_group() {
        assert!(starts_group(&key(0x61, KEYEVENTF_UNICODE)));
        assert!(starts_group(&key(0xD83D, KEYEVENTF_UNICODE)));
        assert!(!starts_group(&key(0xDE00, KEYEVENTF_UNICODE)));
        assert!(!starts_group(&key(0x61, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP)));
        assert!(starts_group(&mouse(MOUSEEVENTF_MOVE)));
        assert!(starts_group(&mouse(MOUSEEVENTF_LEFTDOWN)));
        assert!(!starts_group(&mouse(MOUSEEVENTF_LEFTUP)));
    }

    #[test]
    fn a_group_longer_than_the_limit_is_cut_at_the_limit() {
        let mut events = vec![key(0x61, KEYBD_EVENT_FLAGS(0))];
        events.extend(std::iter::repeat_n(key(0x61, KEYEVENTF_KEYUP), 600));
        assert_eq!(sizes(&events), vec![512, 89]);
    }

    #[test]
    fn empty_input_needs_no_calls() {
        assert!(split_into_calls(&[]).is_empty());
    }
}
