use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use deckhand_core::{Result, keys::Chord};
use windows::Win32::UI::{
    Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey},
    WindowsAndMessaging::{MSG, WM_HOTKEY, WM_TIMER},
};

use crate::{
    hotkey_binding::{HotkeyBinding, hotkey_binding},
    layout::SystemLayout,
    message_thread::{MessageThread, ThreadMessages},
    thread_timer::ThreadTimer,
};

const HOTKEY_ID: i32 = 0x4448;
const RETRY_INTERVAL_MS: u32 = 2_000;

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
        let (thread, ()) = MessageThread::spawn("deckhand-hotkey", move || {
            HotkeyLoop::start(binding, shared, on_press).map(|state| (state, ()))
        })?;
        Ok(Self { registered, _thread: thread })
    }

    pub fn is_registered(&self) -> bool {
        self.registered.load(Ordering::Acquire)
    }
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

impl<F: Fn()> ThreadMessages for HotkeyLoop<F> {
    fn on_message(&mut self, message: &MSG) -> bool {
        match message.message {
            WM_HOTKEY if message.wParam.0 == HOTKEY_ID as usize => (self.on_press)(),
            WM_TIMER if self.is_retry_timer(message.wParam.0) => self.retry(),
            _ => return false,
        }
        true
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

    use deckhand_core::keys::{Key, Modifiers};

    use super::*;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn listener_can_be_shared_across_threads() {
        assert_send_sync::<HotkeyListener>();
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
