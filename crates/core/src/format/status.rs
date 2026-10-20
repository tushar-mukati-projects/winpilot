use crate::reports::{StatusReport, Timing};

use super::MS_PER_SECOND;

const RECENT_TIMINGS: usize = 5;

pub fn status(report: &StatusReport) -> String {
    let mut lines = vec![format!("version: {}", report.version)];
    lines.extend(report.server_uptime_ms.map(|ms| format!("server uptime: {}", seconds(ms))));
    lines.extend(report.worker_uptime_ms.map(|ms| format!("worker uptime: {}", seconds(ms))));
    lines.extend([
        stopped_line(report),
        format!("user idle: {}", report.user_idle_ms.map_or_else(|| "unknown".to_string(), seconds)),
        hotkey_line(report),
        format!("elevated: {}", yes_no(report.elevated)),
        format!("secure desktop: {}", yes_no(report.secure_desktop)),
        format!("worker restarts: {}", report.worker_restarts),
    ]);
    lines.extend(recent_line(&report.recent));
    lines.join("\n")
}

fn seconds(ms: u64) -> String {
    format!("{:.1} s", ms as f64 / MS_PER_SECOND as f64)
}

const fn yes_no(flag: bool) -> &'static str {
    if flag { "yes" } else { "no" }
}

fn stopped_line(report: &StatusReport) -> String {
    if report.stopped {
        return format!("stopped: yes — press {} again to resume", report.kill_hotkey);
    }
    "stopped: no".to_string()
}

fn hotkey_line(report: &StatusReport) -> String {
    let suffix = if report.hotkey_registered { "" } else { " (not registered)" };
    format!("kill hotkey: {}{suffix}", report.kill_hotkey)
}

fn recent_line(recent: &[Timing]) -> Option<String> {
    if recent.is_empty() {
        return None;
    }
    let skipped = recent.len().saturating_sub(RECENT_TIMINGS);
    let timings: Vec<String> = recent.iter().skip(skipped).map(timing).collect();
    Some(format!("recent: {}", timings.join(", ")))
}

fn timing(timing: &Timing) -> String {
    let outcome = if timing.ok { "ok" } else { "err" };
    format!("{} {} ms {outcome}", timing.tool, timing.ms)
}
