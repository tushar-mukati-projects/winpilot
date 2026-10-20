use winpilot_core::protocol::StatusReport;

pub fn server_report(kill_hotkey: &str) -> StatusReport {
    StatusReport {
        version: crate::VERSION.to_string(),
        server_uptime_ms: None,
        worker_uptime_ms: None,
        stopped: false,
        user_idle_ms: None,
        kill_hotkey: kill_hotkey.to_string(),
        hotkey_registered: false,
        elevated: false,
        secure_desktop: false,
        worker_restarts: 0,
        recent: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use winpilot_core::format;

    use super::*;

    #[test]
    fn server_report_knows_only_what_the_server_knows() {
        let report = server_report("ctrl+alt+escape");
        assert_eq!(report.version, crate::VERSION);
        assert_eq!(report.worker_uptime_ms, None);
        let text = format::status(&report);
        assert!(!text.contains("worker uptime"), "{text}");
        assert!(text.contains("user idle: unknown"), "{text}");
        assert!(text.contains("kill hotkey: ctrl+alt+escape (not registered)"), "{text}");
    }
}
