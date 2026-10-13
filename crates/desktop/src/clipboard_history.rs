use windows::{
    Win32::System::DataExchange::RegisterClipboardFormatW,
    core::{PCWSTR, w},
};
use winpilot_core::{Context, Result};

use crate::{clipboard::ClipboardData, global_memory::OwnedGlobal};

const EXCLUDE_FROM_MONITORS: PCWSTR = w!("ExcludeClipboardContentFromMonitorProcessing");
const CAN_INCLUDE_IN_HISTORY: PCWSTR = w!("CanIncludeInClipboardHistory");
const CAN_UPLOAD_TO_CLOUD: PCWSTR = w!("CanUploadToCloudClipboard");
const VIEWER_IGNORE: PCWSTR = w!("Clipboard Viewer Ignore");
const NOT_ALLOWED: u32 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PrivacyFormats {
    exclude_from_monitors: u32,
    viewer_ignore: u32,
    can_include_in_history: u32,
}

impl PrivacyFormats {
    pub(crate) fn registered() -> Result<Self> {
        Ok(Self {
            exclude_from_monitors: registered_format(EXCLUDE_FROM_MONITORS)?,
            viewer_ignore: registered_format(VIEWER_IGNORE)?,
            can_include_in_history: registered_format(CAN_INCLUDE_IN_HISTORY)?,
        })
    }

    pub(crate) fn marks_private(&self, formats: &[u32], value_of: impl Fn(u32) -> Option<u32>) -> bool {
        if formats.contains(&self.exclude_from_monitors) || formats.contains(&self.viewer_ignore) {
            return true;
        }
        formats.contains(&self.can_include_in_history) && value_of(self.can_include_in_history) == Some(NOT_ALLOWED)
    }
}

pub(crate) fn history_exclusion_markers() -> Result<Vec<ClipboardData>> {
    [EXCLUDE_FROM_MONITORS, CAN_INCLUDE_IN_HISTORY, CAN_UPLOAD_TO_CLOUD].into_iter().map(not_allowed_marker).collect()
}

fn not_allowed_marker(name: PCWSTR) -> Result<ClipboardData> {
    let format = registered_format(name)?;
    let memory = OwnedGlobal::with_bytes(&NOT_ALLOWED.to_le_bytes())?;
    Ok(ClipboardData { format, memory })
}

fn registered_format(name: PCWSTR) -> Result<u32> {
    let format = unsafe { RegisterClipboardFormatW(name) };
    if format == 0 {
        return Err(windows::core::Error::from_thread()).context("registering a clipboard format");
    }
    Ok(format)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_use_three_distinct_registered_formats() {
        let markers = history_exclusion_markers().unwrap();
        let mut formats: Vec<u32> = markers.iter().map(|marker| marker.format).collect();
        assert!(formats.iter().all(|&format| format >= 0xC000));
        formats.dedup();
        assert_eq!(formats.len(), 3);
    }

    #[test]
    fn a_password_manager_copy_is_recognised_as_private() {
        let privacy = PrivacyFormats::registered().unwrap();
        let text = 13;
        let unknown = |_| None;
        assert!(!privacy.marks_private(&[text], unknown));
        assert!(privacy.marks_private(&[text, privacy.exclude_from_monitors], unknown));
        assert!(privacy.marks_private(&[privacy.viewer_ignore, text], unknown));
        let history = privacy.can_include_in_history;
        assert!(privacy.marks_private(&[text, history], |_| Some(0)));
        assert!(!privacy.marks_private(&[text, history], |_| Some(1)));
        assert!(!privacy.marks_private(&[text, history], unknown));
    }

    #[test]
    fn registered_formats_are_stable() {
        assert_eq!(
            registered_format(CAN_INCLUDE_IN_HISTORY).unwrap(),
            registered_format(CAN_INCLUDE_IN_HISTORY).unwrap()
        );
    }
}
