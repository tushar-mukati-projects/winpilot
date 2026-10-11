use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use deckhand_core::{
    Error, Result,
    keys::{Chord, Modifiers},
};
use windows::Win32::UI::{
    Input::KeyboardAndMouse::{
        HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey, UnregisterHotKey,
        VIRTUAL_KEY,
    },
    WindowsAndMessaging::{MSG, WM_HOTKEY, WM_TIMER},
};

use crate::{
    keymap::{ResolvedKey, merge_modifiers, not_on_layout, resolve_key},
    layout::{KeyboardLayout, SystemLayout},
    message_thread::{MessageLoop, MessageThread},
    thread_timer::ThreadTimer,
};

const HOTKEY_ID: i32 = 0x4448;
const RETRY_INTERVAL_MS: u32 = 2_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HotkeyBinding {
    modifiers: HOT_KEY_MODIFIERS,
    vk: VIRTUAL_KEY,
}

#[derive(Debug)]
pub struct HotkeyListener {
    registered: Arc<AtomicBool>,
    _thread: MessageThread,
}

impl HotkeyListener {
    pub fn spawn(chord: Chord, on_press: impl Fn() + Send + 'static) -> Result<Self> {
        let binding = hotkey_binding(&chord, &SystemLayout::of_foreground_window())?;
        let registered = Arc::new(AtomicBool::new(false));
        let shared = Arc::clone(&registered);
        let thread = MessageThread::spawn("deckhand-hotkey", move || HotkeyLoop::start(binding, shared, on_press))?;
        Ok(Self { registered, _thread: thread })
    }

    pub fn is_registered(&self) -> bool {
        self.registered.load(Ordering::Acquire)
    }
}

fn hotkey_binding(chord: &Chord, layout: &impl KeyboardLayout) -> Result<HotkeyBinding> {
    let Some(key) = chord.key else {
        return Err(Error::invalid("a hotkey needs a key, not just modifiers"));
    };
    match resolve_key(key, layout)? {
        ResolvedKey::Virtual(found) => {
            let modifiers = hotkey_modifiers(merge_modifiers(chord.modifiers, found.modifiers));
            Ok(HotkeyBinding { modifiers, vk: found.vk })
        }
        ResolvedKey::Unicode(c) => Err(not_on_layout(c)),
    }
}

fn hotkey_modifiers(modifiers: Modifiers) -> HOT_KEY_MODIFIERS {
    [(modifiers.ctrl, MOD_CONTROL), (modifiers.alt, MOD_ALT), (modifiers.shift, MOD_SHIFT), (modifiers.win, MOD_WIN)]
        .into_iter()
        .filter(|(held, _)| *held)
        .fold(MOD_NOREPEAT, |all, (_, flag)| all | flag)
}

struct HotkeyLoop<F> {
    binding: HotkeyBinding,
    registration: Option<Registration>,
    retry_timer: Option<ThreadTimer>,
    registered: Arc<AtomicBool>,
    on_press: F,
}

impl<F: Fn()> HotkeyLoop<F> {
    fn start(binding: HotkeyBinding, registered: Arc<AtomicBool>, on_press: F) -> Result<Self> {
        let mut state = Self { binding, registration: None, retry_timer: None, registered, on_press };
        state.try_register();
        if state.registration.is_none() {
            state.retry_timer = Some(ThreadTimer::start(RETRY_INTERVAL_MS)?);
        }
        Ok(state)
    }

    fn try_register(&mut self) {
        self.registration = Registration::register(self.binding);
        self.registered.store(self.registration.is_some(), Ordering::Release);
    }

    fn retry(&mut self) {
        self.try_register();
        if self.registration.is_some() {
            self.retry_timer = None;
        }
    }

    fn is_retry_timer(&self, timer_id: usize) -> bool {
        self.retry_timer.as_ref().is_some_and(|timer| timer.id() == timer_id)
    }
}

impl<F: Fn()> MessageLoop for HotkeyLoop<F> {
    fn on_message(&mut self, message: &MSG) {
        match message.message {
            WM_HOTKEY if message.wParam.0 == HOTKEY_ID as usize => (self.on_press)(),
            WM_TIMER if self.is_retry_timer(message.wParam.0) => self.retry(),
            _ => {}
        }
    }
}

impl<F> Drop for HotkeyLoop<F> {
    fn drop(&mut self) {
        self.registration = None;
        self.registered.store(false, Ordering::Release);
    }
}

struct Registration {
    id: i32,
}

impl Registration {
    fn register(binding: HotkeyBinding) -> Option<Self> {
        let outcome = unsafe { RegisterHotKey(None, HOTKEY_ID, binding.modifiers, u32::from(binding.vk.0)) };
        outcome.ok().map(|()| Self { id: HOTKEY_ID })
    }
}

impl Drop for Registration {
    fn drop(&mut self) {
        let _ = unsafe { UnregisterHotKey(None, self.id) };
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::atomic::AtomicUsize,
        time::{Duration, Instant},
    };

    use deckhand_core::{
        ErrorCode,
        keys::{Key, NamedKey},
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_BACK, VK_F9};

    use super::*;
    use crate::test_layout::FakeLayout;

    fn assert_send_sync<T: Send + Sync>() {}

    fn binding(modifiers: Modifiers, key: Option<Key>) -> Result<HotkeyBinding> {
        hotkey_binding(&Chord { modifiers, key }, &FakeLayout)
    }

    #[test]
    fn listener_can_be_shared_across_threads() {
        assert_send_sync::<HotkeyListener>();
    }

    #[test]
    fn default_kill_switch_maps_to_ctrl_alt_shift_backspace() {
        let modifiers = Modifiers { ctrl: true, alt: true, shift: true, win: false };
        let found = binding(modifiers, Some(Key::Named(NamedKey::Backspace))).unwrap();
        assert_eq!(found.vk, VK_BACK);
        assert_eq!(found.modifiers, MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_NOREPEAT);
    }

    #[test]
    fn win_function_key_maps_with_norepeat() {
        let found = binding(Modifiers { win: true, ..Modifiers::NONE }, Some(Key::Function(9))).unwrap();
        assert_eq!(found, HotkeyBinding { modifiers: MOD_WIN | MOD_NOREPEAT, vk: VK_F9 });
    }

    #[test]
    fn character_shift_state_is_merged() {
        let found = binding(Modifiers { ctrl: true, ..Modifiers::NONE }, Some(Key::Char('A'))).unwrap();
        assert_eq!(found, HotkeyBinding { modifiers: MOD_CONTROL | MOD_SHIFT | MOD_NOREPEAT, vk: VIRTUAL_KEY(0x41) });
    }

    #[test]
    fn modifier_only_chord_is_rejected() {
        let error = binding(Modifiers { ctrl: true, alt: true, ..Modifiers::NONE }, None).unwrap_err();
        assert_eq!(error.code, ErrorCode::InvalidArgument);
    }

    #[test]
    fn unmappable_keys_are_rejected() {
        let off_layout = binding(Modifiers { ctrl: true, ..Modifiers::NONE }, Some(Key::Char('€'))).unwrap_err();
        assert_eq!(off_layout.code, ErrorCode::InvalidArgument);
        let bad_function = binding(Modifiers::NONE, Some(Key::Function(0))).unwrap_err();
        assert_eq!(bad_function.code, ErrorCode::InvalidArgument);
    }

    #[test]
    #[ignore = "registers a system-wide hotkey"]
    fn listener_registers_and_releases_an_unusual_chord() {
        let chord = Chord {
            modifiers: Modifiers { ctrl: true, alt: true, shift: true, win: true },
            key: Some(Key::Function(24)),
        };
        let presses = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&presses);
        let listener = HotkeyListener::spawn(chord, move || {
            counter.fetch_add(1, Ordering::SeqCst);
        })
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !listener.is_registered() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(listener.is_registered());
        drop(listener);
        assert_eq!(presses.load(Ordering::SeqCst), 0);
    }
}
