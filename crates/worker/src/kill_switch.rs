use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use deckhand_core::{Error, keys::parse_chord};
use deckhand_desktop::{overlay::Overlay, sync::NamedEvent};
use deckhand_input::{HotkeyListener, PhysicalHotkey};
use parking_lot::Mutex;

const SAME_PRESS: Duration = Duration::from_millis(500);

pub struct KillSwitch {
    hotkey: Option<HotkeyListener>,
    _physical: Option<PhysicalHotkey>,
}

struct Toggle {
    stop: Arc<NamedEvent>,
    overlay: Option<Arc<Overlay>>,
    last_press: Mutex<Option<Instant>>,
}

impl KillSwitch {
    pub fn spawn(spec: &str, stop: &Arc<NamedEvent>, overlay: Option<Arc<Overlay>>) -> Self {
        let Some(chord) = parse_chord(spec).inspect_err(|error| warn("kill hotkey", error)).ok() else {
            return Self { hotkey: None, _physical: None };
        };
        let toggle = Arc::new(Toggle { stop: Arc::clone(stop), overlay, last_press: Mutex::new(None) });
        let registered = Arc::clone(&toggle);
        let hotkey = HotkeyListener::spawn(chord, move || registered.pressed(Instant::now()))
            .inspect_err(|error| warn("kill hotkey", error))
            .ok();
        let physical = PhysicalHotkey::watch(chord, move || toggle.pressed(Instant::now()))
            .inspect_err(|error| warn("physical kill hotkey", error))
            .ok();
        Self { hotkey, _physical: physical }
    }

    pub fn registered(&self) -> bool {
        self.hotkey.as_ref().is_some_and(HotkeyListener::is_registered)
    }
}

impl Toggle {
    fn pressed(&self, now: Instant) {
        if !first_report(&mut self.last_press.lock(), now) {
            return;
        }
        let stopping = !self.stop.is_set();
        let result = if stopping { self.stop.set() } else { self.stop.reset() };
        match result {
            Ok(()) => announce(self.overlay.as_deref(), stopping),
            Err(error) => warn("kill switch", &error),
        }
    }
}

fn first_report(last_press: &mut Option<Instant>, now: Instant) -> bool {
    let repeated = last_press.is_some_and(|earlier| now.saturating_duration_since(earlier) < SAME_PRESS);
    if !repeated {
        *last_press = Some(now);
    }
    !repeated
}

fn announce(overlay: Option<&Overlay>, stopping: bool) {
    let Some(overlay) = overlay else { return };
    if stopping {
        overlay.stopped();
    } else {
        overlay.resumed();
    }
}

fn warn(what: &str, error: &Error) {
    eprintln!("deckhand worker: {what} unavailable: {error}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hotkey_and_the_physical_chord_count_as_one_press() {
        let start = Instant::now();
        let mut last = None;
        assert!(first_report(&mut last, start));
        assert!(!first_report(&mut last, start + Duration::from_millis(20)));
        assert!(!first_report(&mut last, start + Duration::from_millis(499)));
        assert!(first_report(&mut last, start + Duration::from_millis(600)));
    }

    #[test]
    fn an_empty_hotkey_watches_nothing() {
        let stop = Arc::new(NamedEvent::create("Local\\deckhand-kill-switch-test").unwrap());
        let switch = KillSwitch::spawn("", &stop, None);
        assert!(switch.hotkey.is_none() && !switch.registered());
    }
}
