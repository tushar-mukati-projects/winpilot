use crate::{
    element::{WindowInfo, WindowState},
    geometry::Rect,
    reports::{AppInfo, AppKind, AppsReport, MonitorInfo, MonitorsReport, WindowsReport},
};

use super::{
    NAME_LIMIT,
    quoting::{quote, single_line},
};

pub(super) const APP_ID_LIMIT: usize = 160;
const MAX_LISTED_WINDOWS: usize = 8;

pub fn windows(report: &WindowsReport) -> String {
    listed(&report.windows.iter().map(window_line).collect::<Vec<_>>(), "no windows")
}

pub fn monitors(report: &MonitorsReport) -> String {
    listed(&report.monitors.iter().map(monitor_line).collect::<Vec<_>>(), "no monitors")
}

pub fn apps(report: &AppsReport) -> String {
    let mut lines: Vec<String> = report.note.iter().cloned().collect();
    lines.extend(report.apps.iter().map(app_line));
    listed(&lines, "no apps")
}

fn listed(lines: &[String], when_empty: &str) -> String {
    if lines.is_empty() {
        return when_empty.to_string();
    }
    lines.join("\n")
}

fn monitor_line(monitor: &MonitorInfo) -> String {
    let primary = if monitor.primary { " primary" } else { "" };
    format!(
        "monitor {} {} {}%{primary} · work area {}",
        monitor.index,
        size_at(monitor.rect),
        monitor.scale_percent,
        size_at(monitor.work_area)
    )
}

pub(super) fn app_line(app: &AppInfo) -> String {
    let kind = match app.kind {
        AppKind::Packaged => "packaged",
        AppKind::Desktop => "desktop",
    };
    format!("{} {kind} {}", quote(app.name.trim(), NAME_LIMIT), single_line(app.id.trim(), APP_ID_LIMIT))
}

pub fn window_line(window: &WindowInfo) -> String {
    let mut parts = vec![window_tag(window), quote(window.title.trim(), NAME_LIMIT)];
    parts.extend(window_flags(window).map(str::to_string));
    parts.extend(other_desktop(window));
    parts.join(" ")
}

pub fn other_desktop(window: &WindowInfo) -> Option<String> {
    if window.on_current_desktop {
        return None;
    }
    Some(window.desktop.map_or_else(|| "another desktop".to_string(), |number| format!("desktop {number}")))
}

pub(super) fn window_tag(window: &WindowInfo) -> String {
    let process = window.process.trim();
    if process.is_empty() {
        return window.hwnd.to_string();
    }
    format!("{} {}", window.hwnd, single_line(process, NAME_LIMIT))
}

fn window_flags(window: &WindowInfo) -> impl Iterator<Item = &'static str> {
    let placement = match window.state {
        WindowState::Minimized => Some("minimized"),
        WindowState::Maximized => Some("maximized"),
        WindowState::Normal => None,
    };
    [placement, window.elevated.then_some("elevated"), window.foreground.then_some("foreground")].into_iter().flatten()
}

pub(super) fn window_list(label: &str, windows: &[WindowInfo]) -> Option<String> {
    if windows.is_empty() {
        return None;
    }
    let mut parts: Vec<String> = windows.iter().take(MAX_LISTED_WINDOWS).map(window_line).collect();
    if windows.len() > MAX_LISTED_WINDOWS {
        parts.push(format!("… {} more", windows.len() - MAX_LISTED_WINDOWS));
    }
    Some(format!("{label}: {}", parts.join(", ")))
}

pub(super) fn size_at(rect: Rect) -> String {
    let Rect { x, y, width, height } = rect;
    format!("{width}x{height}@{x},{y}")
}
