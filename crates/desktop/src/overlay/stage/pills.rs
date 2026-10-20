use winpilot_core::Rect;

use super::{Stage, pill_layer::PillShow};
use crate::overlay::{monitor_at::work_area_around, pill_kind::PillKind, pill_label::PresenceLook, scene_frame::Frame};

const RECORDING_SPIN: f32 = 0.25;

impl Stage {
    pub(super) fn show_presence(&mut self, frame: &Frame<'_>, screen: Rect, paint_tick: bool) {
        let Some(pill) = frame.pill else {
            self.pill.hide();
            return;
        };
        self.pill.show(
            &PillShow {
                kind: PillKind::Presence(PresenceLook {
                    tone: pill.tone,
                    driving: pill.driving,
                    room: pill.label_anchor.width,
                }),
                anchor: pill.anchor,
                scale: pill.scale,
                alpha: pill.alpha,
                pulse: pill.pulse,
                spin: pill.spin,
                entrance: pill.entrance,
                repaint: paint_tick || !pill.steady,
            },
            work_area_around(pill.anchor).unwrap_or(screen),
        );
    }

    pub(super) fn show_activity(&mut self, frame: &Frame<'_>, paint_tick: bool) {
        let Some(activity) = frame.activity else {
            self.activity.hide();
            return;
        };
        self.activity.describe(activity.hint);
        self.activity.show(
            &PillShow {
                kind: PillKind::Activity,
                anchor: activity.anchor,
                scale: activity.scale,
                alpha: activity.alpha,
                pulse: activity.pulse,
                spin: activity.spin,
                entrance: activity.entrance,
                repaint: paint_tick || !activity.steady,
            },
            activity.anchor,
        );
    }

    pub(super) fn show_recording(&mut self, frame: &Frame<'_>, screen: Rect, paint_tick: bool) {
        let Some(recording) = frame.recording else {
            self.recording.hide();
            return;
        };
        self.recording.show(
            &PillShow {
                kind: PillKind::Recording { seconds: recording.seconds },
                anchor: recording.rect,
                scale: recording.scale,
                alpha: 1.0,
                pulse: recording.pulse,
                spin: recording.pulse * RECORDING_SPIN,
                entrance: 1.0,
                repaint: paint_tick,
            },
            work_area_around(recording.rect).unwrap_or(screen),
        );
    }
}
