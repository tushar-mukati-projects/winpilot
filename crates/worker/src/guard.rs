use std::{
    path::{Path, PathBuf},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use deckhand_capture::StopCheck;
use deckhand_core::{Context, Error, ErrorCode, Result, WindowInfo, config::GuardConfig};
use deckhand_desktop::{
    overlay::Overlay,
    sync::{NamedEvent, NamedMutex, NamedMutexGuard},
};
use deckhand_input::ActivityMonitor;

use crate::{
    call_scope::CallScope,
    kill_switch::{KillSwitch, warn},
};

const STOP_EVENT: &str = "Local\\deckhand-stopped";
const ACTOR_MUTEX: &str = "Local\\deckhand-actor";
const ACTOR_WAIT: Duration = Duration::from_secs(10);
const YIELD_POLL: Duration = Duration::from_millis(50);
const BUSY: &str = "another Deckhand call or agent session is controlling the desktop";
const RELEASING_STUCK_INPUTS: &str = "releasing keys and buttons that a crashed Deckhand call left held down";

pub(crate) struct Guard {
    config: GuardConfig,
    stop: Arc<NamedEvent>,
    actor: NamedMutex,
    activity: Option<ActivityMonitor>,
    kill_switch: KillSwitch,
    elevated: bool,
}

impl Guard {
    pub(crate) fn start(config: &GuardConfig, overlay: Option<Arc<Overlay>>) -> Result<Self> {
        let stop = Arc::new(NamedEvent::create(STOP_EVENT)?);
        let actor = NamedMutex::create(ACTOR_MUTEX)?;
        let activity = ActivityMonitor::spawn().inspect_err(|error| warn("user-activity monitor", error)).ok();
        let kill_switch = KillSwitch::spawn(&config.kill_hotkey, &stop, overlay);
        let elevated = deckhand_desktop::own_process_elevated();
        Ok(Self { config: config.clone(), stop, actor, activity, kill_switch, elevated })
    }

    pub(crate) fn is_stopped(&self) -> bool {
        self.stop.is_set()
    }

    pub(crate) fn kill_switch_check(&self) -> StopCheck {
        let stop = Arc::clone(&self.stop);
        Box::new(move || stop.is_set())
    }

    pub(crate) fn kill_hotkey(&self) -> &str {
        &self.config.kill_hotkey
    }

    pub(crate) fn hotkey_registered(&self) -> bool {
        self.kill_switch.registered()
    }

    pub(crate) fn user_idle(&self) -> Option<Duration> {
        self.activity.as_ref().and_then(ActivityMonitor::idle_for)
    }

    pub(crate) fn physical_input_since(&self, since: Instant) -> bool {
        let last = self.activity.as_ref().and_then(ActivityMonitor::last_physical_input);
        last.is_some_and(|at| at > since)
    }

    pub(crate) fn own_process_elevated(&self) -> bool {
        self.elevated
    }

    pub(crate) fn denies(&self, window: &WindowInfo) -> bool {
        self.denies_titled(&window.process, &window.title)
    }

    pub(crate) fn denies_titled(&self, process: &str, title: &str) -> bool {
        self.config.denies_window(process, title)
    }

    pub(crate) fn check_window(&self, window: &WindowInfo) -> Result<()> {
        if self.denies(window) {
            return Err(Error::new(
                ErrorCode::Denied,
                format!("{} ({}) is on Deckhand's deny list", window.hwnd, window.process),
            ));
        }
        Ok(())
    }

    pub(crate) fn checked_path(&self, path: &Path) -> Result<PathBuf> {
        self.config.checked_path(path)
    }

    pub(crate) fn protects_paths(&self) -> bool {
        !self.config.deny_paths.is_empty()
    }

    pub(crate) fn acquire_actor(&self, call: &CallScope) -> Result<NamedMutexGuard<'_>> {
        let wait = call.bounded(ACTOR_WAIT);
        let Some(actor) = self.actor.lock(wait)? else { return Err(busy(wait < ACTOR_WAIT)) };
        call.note_acting();
        if actor.was_abandoned() {
            deckhand_input::release_stuck_inputs().context(RELEASING_STUCK_INPUTS)?;
        }
        Ok(actor)
    }

    pub(crate) fn release_inputs_a_crash_left_held(&self) -> Result<()> {
        let Some(_recovering) = self.actor.try_lock_abandoned()? else { return Ok(()) };
        deckhand_input::release_stuck_inputs().context(RELEASING_STUCK_INPUTS)
    }

    pub(crate) fn check_running(&self, call: &CallScope) -> Result<()> {
        call.check()?;
        if self.is_stopped() {
            return Err(self.stopped());
        }
        Ok(())
    }

    pub(crate) fn check_waiting(&self, call: &CallScope) -> Result<()> {
        call.check()?;
        if call.is_acting() && self.is_stopped() {
            return Err(self.stopped());
        }
        Ok(())
    }

    fn stopped(&self) -> Error {
        Error::new(
            ErrorCode::StoppedByUser,
            format!("Deckhand is stopped by the kill hotkey; the user can press {} to resume", self.config.kill_hotkey),
        )
    }

    pub(crate) fn before_input(&self, window: Option<&WindowInfo>, call: &CallScope) -> Result<()> {
        self.before_patterns(window, call)?;
        self.yield_to_user(call)
    }

    pub(crate) fn before_patterns(&self, window: Option<&WindowInfo>, call: &CallScope) -> Result<()> {
        self.check_running(call)?;
        if deckhand_desktop::secure_desktop_active() {
            return Err(Error::new(
                ErrorCode::NeedsUser,
                "a UAC prompt or the lock screen is active; the user must handle it",
            ));
        }
        if let Some(window) = window {
            self.check_window(window)?;
            self.check_elevation(window)?;
        }
        Ok(())
    }

    fn check_elevation(&self, window: &WindowInfo) -> Result<()> {
        if window.elevated && !self.elevated {
            return Err(Error::new(
                ErrorCode::ElevatedTarget,
                format!("{} ({}) runs as administrator; Deckhand cannot control it", window.hwnd, window.process),
            ));
        }
        Ok(())
    }

    fn yield_to_user(&self, call: &CallScope) -> Result<()> {
        let quiet = Duration::from_millis(self.config.yield_ms);
        let limit = call.bounded(Duration::from_millis(self.config.yield_timeout_ms));
        let started = Instant::now();
        loop {
            if self.user_idle().is_none_or(|idle| idle >= quiet) {
                return Ok(());
            }
            let left = limit.saturating_sub(started.elapsed());
            if left.is_zero() {
                return Err(Error::new(
                    ErrorCode::UserActive,
                    "the user is using the mouse or keyboard; waiting for them to stop timed out",
                ));
            }
            thread::sleep(YIELD_POLL.min(left));
            self.check_running(call)?;
        }
    }
}

fn busy(cut_short: bool) -> Error {
    if cut_short {
        return Error::timeout(format!("{BUSY}, and this call ran out of time waiting for it"));
    }
    Error::new(ErrorCode::Busy, format!("{BUSY}; try again shortly"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn busy_names_both_kinds_of_holder() {
        let busy_now = busy(false);
        assert_eq!(busy_now.code, ErrorCode::Busy);
        assert!(busy_now.message.starts_with("another Deckhand call or agent session"), "{}", busy_now.message);
        assert_eq!(busy(true).code, ErrorCode::Timeout);
    }
}
