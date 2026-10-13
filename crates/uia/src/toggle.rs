use std::{thread, time::Duration};

use windows::Win32::UI::Accessibility::{IUIAutomationTogglePattern, ToggleState, ToggleState_On, UIA_TogglePatternId};
use winpilot_core::{ElementRef, Result};

use crate::{automation::Uia, failure::UiaResult};

const SETTLE_POLLS: u32 = 10;
const SETTLE_STEP: Duration = Duration::from_millis(20);

impl Uia {
    pub fn toggle(&mut self, id: ElementRef) -> Result<bool> {
        let target = self.resolve(id)?;
        let pattern: IUIAutomationTogglePattern = target.pattern(UIA_TogglePatternId, "Toggle")?;
        let before = read_state(&pattern);
        unsafe { pattern.Toggle() }.uia(&target.doing("toggling"))?;
        Ok(settled_state(before, || read_state(&pattern), || thread::sleep(SETTLE_STEP)))
    }
}

fn read_state(pattern: &IUIAutomationTogglePattern) -> Option<ToggleState> {
    unsafe { pattern.CurrentToggleState() }.ok()
}

fn settled_state(
    before: Option<ToggleState>,
    mut read: impl FnMut() -> Option<ToggleState>,
    mut pause: impl FnMut(),
) -> bool {
    let mut latest = None;
    for poll in 0..=SETTLE_POLLS {
        if poll > 0 {
            pause();
        }
        latest = read().or(latest);
        if let Some(changed) = latest.filter(|state| Some(*state) != before) {
            return is_on(changed);
        }
    }
    latest.map_or_else(|| !before.is_some_and(is_on), is_on)
}

fn is_on(state: ToggleState) -> bool {
    state == ToggleState_On
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, collections::VecDeque};

    use windows::Win32::UI::Accessibility::{ToggleState_Indeterminate, ToggleState_Off};

    use super::*;

    fn settle(before: Option<ToggleState>, reads: &[Option<ToggleState>]) -> (bool, u32) {
        let mut queue: VecDeque<Option<ToggleState>> = reads.iter().copied().collect();
        let last = reads.last().copied().flatten();
        let pauses = Cell::new(0);
        let state = settled_state(before, || queue.pop_front().unwrap_or(last), || pauses.set(pauses.get() + 1));
        (state, pauses.get())
    }

    #[test]
    fn waits_for_a_late_state_change() {
        let reads = [Some(ToggleState_Off), Some(ToggleState_Off), Some(ToggleState_On)];
        assert_eq!(settle(Some(ToggleState_Off), &reads), (true, 2));
    }

    #[test]
    fn an_unchanged_state_is_reported_as_it_is_after_200_ms() {
        assert_eq!(settle(Some(ToggleState_Off), &[Some(ToggleState_Off)]), (false, SETTLE_POLLS));
    }

    #[test]
    fn an_unreadable_new_state_is_the_opposite_of_before() {
        assert_eq!(settle(Some(ToggleState_Off), &[None]), (true, SETTLE_POLLS));
        assert_eq!(settle(Some(ToggleState_On), &[None]), (false, SETTLE_POLLS));
        assert_eq!(settle(Some(ToggleState_Indeterminate), &[None]), (true, SETTLE_POLLS));
        assert_eq!(settle(None, &[None]), (true, SETTLE_POLLS));
    }

    #[test]
    fn an_unknown_before_takes_the_first_reading() {
        assert_eq!(settle(None, &[Some(ToggleState_On)]), (true, 0));
        assert_eq!(settle(None, &[None, Some(ToggleState_Off)]), (false, 1));
    }

    #[test]
    fn indeterminate_counts_as_not_on() {
        assert_eq!(settle(Some(ToggleState_On), &[Some(ToggleState_Indeterminate)]), (false, 0));
        assert!(!is_on(ToggleState_Indeterminate));
    }

    #[test]
    fn a_failed_read_keeps_the_last_good_one() {
        let reads = [Some(ToggleState_Off), None, None];
        assert_eq!(settle(Some(ToggleState_Off), &reads), (false, SETTLE_POLLS));
    }
}
