use deckhand_core::Rect;
use deckhand_desktop::overlay::Overlay;

pub fn show_recording_indicator(overlay: Option<&Overlay>, recording: Option<Rect>) {
    match (overlay, recording) {
        (Some(overlay), Some(area)) => overlay.recording(area),
        (Some(overlay), None) => overlay.recording_stopped(),
        (None, _) => {}
    }
}
