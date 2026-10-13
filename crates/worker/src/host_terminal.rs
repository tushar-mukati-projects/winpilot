use winpilot_core::{
    Error, ErrorCode, Point, Result, WindowInfo,
    host_marks::{is_opt_in_marker, is_protected_id},
};

use crate::{
    engine::Engine,
    resolve::{Reach, Resolved},
};

pub(crate) const KEYS_IN_HOST: &str =
    "key presses never go to the agent's own app; click a control or type into a specific field instead";

pub(crate) fn check_target(engine: &Engine, resolved: &Resolved) -> Result<()> {
    let window = &resolved.window;
    if !engine.host.contains(window.pid) {
        return Ok(());
    }
    if !opted_in(engine, window)? {
        return engine.host.refuse(window);
    }
    let element = match &resolved.reach {
        Reach::Element(element) => element.id,
        Reach::Point { under, .. } => under.id,
        Reach::Window => return Err(whole_window(window)),
    };
    match engine.uia.run(move |uia| uia.marked_ancestor(element, is_protected_id))? {
        Some(protected) => Err(inside_terminal(&protected)),
        None => Ok(()),
    }
}

pub(crate) fn check_point(engine: &Engine, window: &WindowInfo, point: Point) -> Result<()> {
    if !engine.host.contains(window.pid) {
        return Ok(());
    }
    let (hit, owner) = engine.uia.run(move |uia| {
        let hit = uia.element_at(point)?;
        uia.window_of(hit.id).map(|owner| (hit.id, owner))
    })?;
    if owner != window.hwnd {
        return Err(hidden_spot(window, point));
    }
    match engine.uia.run(move |uia| uia.marked_ancestor(hit, is_protected_id))? {
        Some(protected) => Err(inside_terminal(&protected)),
        None => Ok(()),
    }
}

pub(crate) fn refuse_keys(engine: &Engine, window: &WindowInfo) -> Result<()> {
    if engine.host.contains(window.pid) {
        return Err(Error::new(ErrorCode::Denied, KEYS_IN_HOST));
    }
    Ok(())
}

fn opted_in(engine: &Engine, window: &WindowInfo) -> Result<bool> {
    if !engine.guard.allows_partial_host(&window.process) {
        return Ok(false);
    }
    let snapshot = engine.snapshot(window)?;
    Ok(snapshot.elements.iter().any(|element| is_opt_in_marker(&element.automation_id)))
}

fn whole_window(window: &WindowInfo) -> Error {
    Error::new(
        ErrorCode::Denied,
        format!("{} is the agent's own app; target a specific control outside its session terminals", window.hwnd),
    )
}

fn hidden_spot(window: &WindowInfo, point: Point) -> Error {
    Error::new(
        ErrorCode::Denied,
        format!(
            "{},{} in {} is covered by another window, so WinPilot can't confirm it is outside the agent's session terminals",
            point.x, point.y, window.hwnd
        ),
    )
}

fn inside_terminal(protected: &str) -> Error {
    Error::new(
        ErrorCode::Denied,
        format!(
            "that spot is inside {protected}, a session terminal of the agent's own app; WinPilot never clicks or types there"
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refusals_name_the_terminal_and_point_to_a_way_out() {
        let terminal = inside_terminal("winpilot-protected-terminal-tm2");
        assert_eq!(terminal.code, ErrorCode::Denied);
        assert!(terminal.message.contains("winpilot-protected-terminal-tm2"), "{}", terminal.message);
        let window =
            crate::test_fixture::window(7, "agent-deck.exe", "Agent Deck", winpilot_core::Rect::new(0, 0, 9, 9));
        let whole = whole_window(&window);
        assert!(whole.message.starts_with("w7 is the agent's own app"), "{}", whole.message);
        let hidden = hidden_spot(&window, Point::new(3, 4));
        assert_eq!(hidden.code, ErrorCode::Denied);
        assert!(hidden.message.starts_with("3,4 in w7 is covered by another window"), "{}", hidden.message);
    }
}
