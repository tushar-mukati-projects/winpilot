use deckhand_core::{Context, Result};
use windows::{
    Win32::System::DataExchange::RegisterClipboardFormatW,
    core::{PCWSTR, w},
};

use crate::{clipboard::ClipboardData, global_memory::OwnedGlobal};

const EXCLUDE_FROM_MONITORS: PCWSTR = w!("ExcludeClipboardContentFromMonitorProcessing");
const CAN_INCLUDE_IN_HISTORY: PCWSTR = w!("CanIncludeInClipboardHistory");
const CAN_UPLOAD_TO_CLOUD: PCWSTR = w!("CanUploadToCloudClipboard");
const NOT_ALLOWED: u32 = 0;

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
    fn registered_formats_are_stable() {
        assert_eq!(
            registered_format(CAN_INCLUDE_IN_HISTORY).unwrap(),
            registered_format(CAN_INCLUDE_IN_HISTORY).unwrap()
        );
    }
}
