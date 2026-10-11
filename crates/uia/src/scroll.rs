use deckhand_core::{ElementRef, Error, Result, protocol::Direction};
use windows::Win32::UI::Accessibility::{
    IUIAutomationElement, IUIAutomationScrollPattern, ScrollAmount, ScrollAmount_NoAmount, ScrollAmount_SmallDecrement,
    ScrollAmount_SmallIncrement, UIA_ScrollPatternId,
};

use crate::{
    automation::Uia,
    failure::UiaResult,
    lookup::MAX_ANCESTRY,
    resolved::{Resolved, current_pattern},
};

const MAX_SCROLL_STEPS: u32 = 200;
const EDGE_TOLERANCE: f64 = 0.01;
const FULL_PERCENT: f64 = 100.0;

impl Uia {
    pub fn scroll(&mut self, id: ElementRef, direction: Direction, amount: u32) -> Result<()> {
        let target = self.resolve(id)?;
        let scroller = self.scroller_for(&target, direction)?;
        let (horizontal, vertical) = step(direction);
        for _ in 0..amount.min(MAX_SCROLL_STEPS) {
            if at_edge(&scroller, direction, &target)? {
                break;
            }
            unsafe { scroller.Scroll(horizontal, vertical) }.uia(&target.doing("scrolling"))?;
        }
        Ok(())
    }

    fn scroller_for(&self, target: &Resolved, direction: Direction) -> Result<IUIAutomationScrollPattern> {
        let mut current = target.element.clone();
        for _ in 0..=MAX_ANCESTRY {
            if let Some(pattern) = scrollable(&current, direction, target)? {
                return Ok(pattern);
            }
            let Some(parent) = self.parent_of(&current)? else { break };
            current = parent;
        }
        Err(Error::unsupported(format!(
            "neither {} nor any container around it can scroll {}",
            target.label(),
            direction_word(direction)
        )))
    }
}

fn scrollable(
    element: &IUIAutomationElement,
    direction: Direction,
    target: &Resolved,
) -> Result<Option<IUIAutomationScrollPattern>> {
    let what = target.doing("looking for a scrollable container around");
    let Some(pattern) = current_pattern::<IUIAutomationScrollPattern>(element, UIA_ScrollPatternId).uia(&what)? else {
        return Ok(None);
    };
    let can_scroll = if is_vertical(direction) {
        unsafe { pattern.CurrentVerticallyScrollable() }
    } else {
        unsafe { pattern.CurrentHorizontallyScrollable() }
    }
    .uia(&what)?;
    Ok(can_scroll.as_bool().then_some(pattern))
}

fn at_edge(pattern: &IUIAutomationScrollPattern, direction: Direction, target: &Resolved) -> Result<bool> {
    let percent = if is_vertical(direction) {
        unsafe { pattern.CurrentVerticalScrollPercent() }
    } else {
        unsafe { pattern.CurrentHorizontalScrollPercent() }
    }
    .uia(&target.doing("reading the scroll position of"))?;
    Ok(is_at_edge(percent, direction))
}

fn is_at_edge(percent: f64, direction: Direction) -> bool {
    if percent < 0.0 {
        return true;
    }
    match direction {
        Direction::Up | Direction::Left => percent <= EDGE_TOLERANCE,
        Direction::Down | Direction::Right => percent >= FULL_PERCENT - EDGE_TOLERANCE,
    }
}

fn is_vertical(direction: Direction) -> bool {
    matches!(direction, Direction::Up | Direction::Down)
}

fn step(direction: Direction) -> (ScrollAmount, ScrollAmount) {
    match direction {
        Direction::Up => (ScrollAmount_NoAmount, ScrollAmount_SmallDecrement),
        Direction::Down => (ScrollAmount_NoAmount, ScrollAmount_SmallIncrement),
        Direction::Left => (ScrollAmount_SmallDecrement, ScrollAmount_NoAmount),
        Direction::Right => (ScrollAmount_SmallIncrement, ScrollAmount_NoAmount),
    }
}

fn direction_word(direction: Direction) -> &'static str {
    match direction {
        Direction::Up => "up",
        Direction::Down => "down",
        Direction::Left => "left",
        Direction::Right => "right",
    }
}

#[cfg(test)]
mod tests {
    use deckhand_core::{ErrorCode, Role};

    use super::*;
    use crate::{SnapshotOptions, test_support::launch_fixture};

    #[test]
    fn edges_depend_on_direction() {
        assert!(is_at_edge(0.0, Direction::Up));
        assert!(!is_at_edge(0.0, Direction::Down));
        assert!(is_at_edge(100.0, Direction::Down));
        assert!(is_at_edge(100.0, Direction::Right));
        assert!(!is_at_edge(50.0, Direction::Left));
        assert!(is_at_edge(-1.0, Direction::Down));
    }

    #[test]
    fn steps_move_one_axis() {
        assert_eq!(step(Direction::Down), (ScrollAmount_NoAmount, ScrollAmount_SmallIncrement));
        assert_eq!(step(Direction::Left), (ScrollAmount_SmallDecrement, ScrollAmount_NoAmount));
        assert!(is_vertical(Direction::Up));
        assert!(!is_vertical(Direction::Right));
    }

    #[test]
    #[ignore = "acts on the deckhand fixture app"]
    fn scrolls_the_listbox_or_reports_unsupported() {
        let Some(fixture) = launch_fixture() else { return };
        let window = fixture.window.clone();
        crate::test_support::handle()
            .run(move |uia| {
                let snapshot = uia.snapshot(&window, &SnapshotOptions::default())?;
                let list = snapshot
                    .elements
                    .iter()
                    .find(|element| element.automation_id == "2009" && element.role == Role::List)
                    .map(|element| element.id)
                    .ok_or_else(|| Error::not_found("listbox 2009"))?;
                match uia.scroll(list, Direction::Down, 2) {
                    Ok(()) => uia.scroll(list, Direction::Up, 2),
                    Err(error) if error.code == ErrorCode::Unsupported => Ok(()),
                    Err(error) => Err(error),
                }
            })
            .unwrap();
    }
}
