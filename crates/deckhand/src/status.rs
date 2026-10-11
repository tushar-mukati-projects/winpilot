use std::time::Duration;

use deckhand_core::protocol::StatusReport;

pub fn server_report(uptime: Duration, kill_hotkey: &str, restarts: u32) -> StatusReport {
    StatusReport {
        version: crate::VERSION.to_string(),
        uptime_ms: u64::try_from(uptime.as_millis()).unwrap_or(u64::MAX),
        stopped: false,
        user_idle_ms: None,
        kill_hotkey: kill_hotkey.to_string(),
        hotkey_registered: false,
        elevated: false,
        secure_desktop: false,
        worker_restarts: restarts,
        recent: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use deckhand_core::format;

    use super::*;

    #[test]
    fn server_report_knows_only_what_the_server_knows() {
        let report = server_report(Duration::from_millis(1500), "ctrl+alt+shift+backspace", 2);
        assert_eq!(report.version, crate::VERSION);
        assert_eq!(report.uptime_ms, 1500);
        assert_eq!(report.worker_restarts, 2);
        let text = format::status(&report);
        assert!(text.contains("user idle: unknown"), "{text}");
        assert!(text.contains("kill hotkey: ctrl+alt+shift+backspace (not registered)"), "{text}");
    }
}
