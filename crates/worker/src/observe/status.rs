use winpilot_core::{millis, protocol::StatusReport};

use crate::engine::Engine;

pub(crate) fn report(engine: &Engine) -> StatusReport {
    StatusReport {
        version: env!("CARGO_PKG_VERSION").to_string(),
        server_uptime_ms: None,
        worker_uptime_ms: Some(millis(engine.uptime())),
        stopped: engine.guard.is_stopped(),
        user_idle_ms: engine.guard.user_idle().map(millis),
        kill_hotkey: engine.guard.kill_hotkey().to_string(),
        hotkey_registered: engine.guard.hotkey_registered(),
        elevated: engine.guard.own_process_elevated(),
        secure_desktop: winpilot_desktop::secure_desktop_active(),
        worker_restarts: 0,
        recent: engine.recent_timings(),
    }
}
