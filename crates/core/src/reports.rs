use serde::{Deserialize, Serialize};

use crate::{
    diff::Change,
    element::{Element, Snapshot, WindowInfo},
    error::Error,
    geometry::{ImageMapping, Rect},
    refs::ImageRef,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    pub snapshot: Snapshot,
    pub others: Vec<WindowInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Found {
    pub element: Element,
    pub window: WindowInfo,
    pub path: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FindReport {
    pub matches: Vec<Found>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScreenshotReport {
    pub image: ImageRef,
    pub mime: String,
    pub data: String,
    pub width: u32,
    pub height: u32,
    pub mapping: ImageMapping,
    pub source: String,
    pub quality: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepOutcome {
    pub index: usize,
    pub step: String,
    pub method: Option<String>,
    pub note: Option<String>,
    pub error: Option<Error>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffReport {
    pub window: WindowInfo,
    pub focused: Option<Element>,
    pub changes: Vec<Change>,
    pub opened: Vec<WindowInfo>,
    pub closed: Vec<WindowInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum After {
    None,
    Diff(DiffReport),
    Observation(Observation),
    Screenshot(ScreenshotReport),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActReport {
    pub steps: Vec<StepOutcome>,
    pub total_steps: usize,
    pub after: After,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppKind {
    Packaged,
    Desktop,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppInfo {
    pub name: String,
    pub id: String,
    pub kind: AppKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MonitorInfo {
    pub index: u32,
    pub rect: Rect,
    pub work_area: Rect,
    pub scale_percent: u32,
    pub primary: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsReport {
    #[serde(default)]
    pub windows: Vec<WindowInfo>,
    #[serde(default)]
    pub monitors: Vec<MonitorInfo>,
    #[serde(default)]
    pub apps: Vec<AppInfo>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextSource {
    Uia,
    Ocr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextReport {
    pub text: String,
    pub source: TextSource,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipboardReport {
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timing {
    pub tool: String,
    pub ms: u64,
    pub ok: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusReport {
    pub version: String,
    pub uptime_ms: u64,
    pub stopped: bool,
    pub user_idle_ms: Option<u64>,
    pub kill_hotkey: String,
    pub hotkey_registered: bool,
    pub elevated: bool,
    pub secure_desktop: bool,
    pub worker_restarts: u32,
    pub recent: Vec<Timing>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_reports_with_only_some_lists_still_deserialize() {
        let report: WindowsReport = serde_json::from_str(r#"{"note":"3 installed apps"}"#).unwrap();
        assert!(report.windows.is_empty() && report.monitors.is_empty() && report.apps.is_empty());
        assert_eq!(report.note.as_deref(), Some("3 installed apps"));
    }
}
