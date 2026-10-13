use winpilot_core::{WindowInfo, WindowRef};

use super::super::{
    by_pattern::KeptForeground, done::both_notes, foreground::owner_chain, stacking::keep_behind_front,
};
use crate::guard::Guard;

const DIALOG_STAYS: &str = "the file dialog stays";

pub(super) struct DialogBehind<'a> {
    kept: KeptForeground<'a>,
}

pub(super) fn dialog_family(dialog: &WindowInfo) -> Vec<WindowRef> {
    let mut family = vec![dialog.hwnd];
    family.extend(owner_chain(Some(dialog.hwnd)));
    family
}

impl<'a> DialogBehind<'a> {
    pub(super) fn record(guard: &'a Guard) -> Self {
        Self { kept: KeptForeground::record(guard) }
    }

    pub(super) fn on_opening(&self, dialog: &WindowInfo, family: &[WindowRef]) -> Option<String> {
        if self.kept.was_in(family) {
            return None;
        }
        let handed_back = self.kept.hand_back(dialog.hwnd);
        both_notes(handed_back, keep_behind_front(dialog, DIALOG_STAYS))
    }

    pub(super) fn after_closing(&self, family: &[WindowRef]) -> Option<String> {
        if self.kept.was_in(family) {
            return None;
        }
        self.kept.hand_back_from(|front| family.contains(&front))
    }
}
