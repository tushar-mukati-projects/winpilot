use deckhand_core::protocol::StatusReport;

use crate::engine::{Engine, duration_ms};

pub fn report(engine: &Engine) -> StatusReport {
    StatusReport {
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_ms: duration_ms(engine.uptime()),
        stopped: engine.guard.is_stopped(),
        user_idle_ms: engine.guard.user_idle().map(duration_ms),
        kill_hotkey: engine.guard.kill_hotkey().to_string(),
        hotkey_registered: engine.guard.hotkey_registered(),
        elevated: engine.guard.own_process_elevated(),
        secure_desktop: deckhand_desktop::secure_desktop_active(),
        worker_restarts: 0,
        recent: engine.recent_timings(),
    }
}
