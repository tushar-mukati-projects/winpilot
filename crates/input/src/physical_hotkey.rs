use std::sync::Arc;

use deckhand_core::{
    Result,
    keys::{Chord, Modifiers},
};
use parking_lot::Mutex;
use windows::Win32::UI::Input::KeyboardAndMouse::{VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT};

use crate::{
    activity::ActivityMonitor,
    chord_detector::{ChordDetector, PhysicalKey, covers, required_modifiers},
    hotkey_binding::{HotkeyBinding, hotkey_binding},
    layout::SystemLayout,
    stuck_inputs::is_logically_down,
};

type Press = Arc<dyn Fn() + Send + Sync>;

struct Watcher {
    id: u64,
    binding: HotkeyBinding,
    on_press: Press,
}

struct Watchers {
    next_id: u64,
    list: Vec<Watcher>,
    detector: ChordDetector,
}

static WATCHERS: Mutex<Watchers> =
    Mutex::new(Watchers { next_id: 1, list: Vec::new(), detector: ChordDetector::new() });

pub struct PhysicalHotkey {
    id: u64,
    _activity: ActivityMonitor,
}

impl PhysicalHotkey {
    pub fn watch(chord: Chord, on_press: impl Fn() + Send + Sync + 'static) -> Result<Self> {
        let binding = hotkey_binding(&chord, &SystemLayout::of_foreground_window())?;
        let activity = ActivityMonitor::spawn()?;
        let mut watchers = WATCHERS.lock();
        let id = watchers.next_id;
        watchers.next_id = watchers.next_id.wrapping_add(1);
        watchers.list.push(Watcher { id, binding, on_press: Arc::new(on_press) });
        Ok(Self { id, _activity: activity })
    }
}

impl Drop for PhysicalHotkey {
    fn drop(&mut self) {
        let mut watchers = WATCHERS.lock();
        watchers.list.retain(|watcher| watcher.id != self.id);
        if watchers.list.is_empty() {
            watchers.detector.forget();
        }
    }
}

pub(crate) fn observe(key: PhysicalKey) {
    for press in presses_for(key, is_logically_down) {
        press();
    }
}

fn presses_for(key: PhysicalKey, is_down: impl Fn(VIRTUAL_KEY) -> bool) -> Vec<Press> {
    let mut watchers = WATCHERS.lock();
    if watchers.list.is_empty() {
        return Vec::new();
    }
    let Some(vk) = watchers.detector.pressed(key) else { return Vec::new() };
    let held = watchers.detector.modifiers();
    watchers
        .list
        .iter()
        .filter(|watcher| watcher.binding.vk == vk)
        .filter(|watcher| chord_is_held(held, required_modifiers(watcher.binding), &is_down))
        .map(|watcher| Arc::clone(&watcher.on_press))
        .collect()
}

fn chord_is_held(physical: Modifiers, wanted: Modifiers, is_down: impl Fn(VIRTUAL_KEY) -> bool) -> bool {
    let logical = Modifiers {
        ctrl: is_down(VK_CONTROL),
        alt: is_down(VK_MENU),
        shift: is_down(VK_SHIFT),
        win: is_down(VK_LWIN) || is_down(VK_RWIN),
    };
    covers(physical, wanted) && covers(logical, wanted)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use windows::Win32::UI::{Input::KeyboardAndMouse::VK_ESCAPE, WindowsAndMessaging::RI_KEY_BREAK};

    use super::*;

    const CTRL_ALT: Modifiers = Modifiers { ctrl: true, alt: true, shift: false, win: false };

    #[test]
    fn a_stale_physical_modifier_is_not_trusted_without_the_logical_state() {
        let everything_down = |_| true;
        let nothing_down = |_| false;
        assert!(chord_is_held(CTRL_ALT, CTRL_ALT, everything_down));
        assert!(!chord_is_held(CTRL_ALT, CTRL_ALT, nothing_down));
        assert!(!chord_is_held(Modifiers::NONE, CTRL_ALT, everything_down));
        let only_ctrl = |vk: VIRTUAL_KEY| vk == VK_CONTROL;
        assert!(!chord_is_held(CTRL_ALT, CTRL_ALT, only_ctrl));
    }

    #[test]
    fn the_watched_chord_fires_once_per_physical_press() {
        let fired = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&fired);
        let binding = hotkey_binding(
            &deckhand_core::keys::parse_chord("ctrl+alt+escape").unwrap(),
            &crate::test_layout::FakeLayout,
        )
        .unwrap();
        let id = {
            let mut watchers = WATCHERS.lock();
            let id = watchers.next_id;
            watchers.next_id += 1;
            let on_press: Press = Arc::new(move || {
                counter.fetch_add(1, Ordering::SeqCst);
            });
            watchers.list.push(Watcher { id, binding, on_press });
            id
        };
        let key = |make_code, vk: VIRTUAL_KEY, up: bool| PhysicalKey {
            make_code,
            flags: if up { RI_KEY_BREAK as u16 } else { 0 },
            vk: vk.0,
        };
        let all_down = |_| true;
        let run = |event| presses_for(event, all_down).into_iter().for_each(|press| press());
        run(key(0x01, VK_ESCAPE, false));
        run(key(0x01, VK_ESCAPE, true));
        assert_eq!(fired.load(Ordering::SeqCst), 0);
        run(key(0x1D, VK_CONTROL, false));
        run(key(0x38, VK_MENU, false));
        run(key(0x01, VK_ESCAPE, false));
        run(key(0x01, VK_ESCAPE, false));
        assert_eq!(fired.load(Ordering::SeqCst), 1);
        for (make_code, vk) in [(0x01, VK_ESCAPE), (0x38, VK_MENU), (0x1D, VK_CONTROL)] {
            run(key(make_code, vk, true));
        }
        WATCHERS.lock().list.retain(|watcher| watcher.id != id);
    }
}
