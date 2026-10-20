use crate::{
    diff::Change,
    element::{Element, ElementState},
    reports::{ActReport, After, DiffReport, StepOutcome},
};

use super::{
    NAME_LIMIT, ObserveFormat, VALUE_LIMIT,
    element::element_line,
    error,
    media::screenshot_caption,
    observation::observation,
    quoting::quote,
    window::{window_line, window_list},
};

const MAX_CHANGES: usize = 40;

pub fn act(report: &ActReport) -> String {
    let mut lines: Vec<String> = report.steps.iter().map(step_line).collect();
    if report.steps.len() < report.total_steps {
        lines.push(format!("stopped after step {} of {}", report.steps.len(), report.total_steps));
    }
    lines.extend(after_lines(&report.after));
    lines.push(format!("took {} ms", report.elapsed_ms));
    lines.join("\n")
}

fn step_line(outcome: &StepOutcome) -> String {
    let number = outcome.index.saturating_add(1);
    if let Some(failure) = &outcome.error {
        return format!("✗ {number} {} — {}", outcome.step, error(failure));
    }
    let method = outcome.method.as_ref().map(|method| format!(" ({method})")).unwrap_or_default();
    let note = outcome.note.as_ref().map(|note| format!(": {note}")).unwrap_or_default();
    format!("✓ {number} {}{method}{note}", outcome.step)
}

fn after_lines(after: &After) -> Vec<String> {
    match after {
        After::None => Vec::new(),
        After::Diff(report) => diff_lines(report),
        After::Observation(found) => vec![format!("after: {}", observation(found, &ObserveFormat::default()))],
        After::Screenshot(shot) => vec![format!("after: {}", screenshot_caption(shot))],
    }
}

fn diff_lines(report: &DiffReport) -> Vec<String> {
    let window = window_line(&report.window);
    let focus = report.focused.as_ref().map(|element| format!(" · focus {}", element_line(element, false)));
    let focus = focus.unwrap_or_default();
    if report.changes.is_empty() && report.opened.is_empty() && report.closed.is_empty() {
        return vec![format!("after: no visible change in {window}{focus}")];
    }
    let mut lines = vec![format!("after: window {window}{focus}")];
    lines.extend(report.changes.iter().take(MAX_CHANGES).map(change_line));
    if report.changes.len() > MAX_CHANGES {
        lines.push(format!("… {} more changes", report.changes.len() - MAX_CHANGES));
    }
    lines.extend(window_list("opened", &report.opened));
    lines.extend(window_list("closed", &report.closed));
    lines
}

pub(super) fn change_line(change: &Change) -> String {
    match change {
        Change::Added { element } => format!("+ {}", element_line(element, false)),
        Change::Removed { element } => format!("- {}", element_line(element, false)),
        Change::Changed { before, after } => changed_line(before, after),
    }
}

fn changed_line(before: &Element, after: &Element) -> String {
    let line = format!("~ {}", element_line(after, false));
    let details = change_details(before, after);
    if details.is_empty() {
        return line;
    }
    format!("{line} ({})", details.join(", "))
}

fn change_details(before: &Element, after: &Element) -> Vec<String> {
    let mut details = Vec::new();
    let (old_name, new_name) = (before.name.trim(), after.name.trim());
    if old_name != new_name {
        details.push(format!("name {} → {}", quote(old_name, NAME_LIMIT), quote(new_name, NAME_LIMIT)));
    }
    details.extend(value_change(before, after));
    if before.role != after.role {
        details.push(format!("role {} → {}", before.role, after.role));
    }
    if before.automation_id != after.automation_id {
        let (old_id, new_id) = (&before.automation_id, &after.automation_id);
        details.push(format!("id {} → {}", quote(old_id, NAME_LIMIT), quote(new_id, NAME_LIMIT)));
    }
    details.extend(state_changes(before.state, after.state).map(str::to_string));
    details
}

fn value_change(before: &Element, after: &Element) -> Option<String> {
    let old = before.value.as_deref().unwrap_or_default();
    let new = after.value.as_deref().unwrap_or_default();
    if old == new {
        return None;
    }
    if before.state.password || after.state.password {
        return Some("value changed".to_string());
    }
    Some(format!("value {} → {}", quote(old, VALUE_LIMIT), quote(new, VALUE_LIMIT)))
}

fn state_changes(before: ElementState, after: ElementState) -> impl Iterator<Item = &'static str> {
    [
        flag_change(before.enabled, after.enabled, "now enabled", "now disabled"),
        option_change(before.toggled, after.toggled, ["now checked", "now unchecked", "no longer checkable"]),
        option_change(before.expanded, after.expanded, ["now expanded", "now collapsed", "no longer expandable"]),
        option_change(before.selected, after.selected, ["now selected", "now unselected", "no longer selectable"]),
        flag_change(before.read_only, after.read_only, "now readonly", "now editable"),
        flag_change(before.password, after.password, "now password", "no longer password"),
        flag_change(before.offscreen, after.offscreen, "now offscreen", "now onscreen"),
        flag_change(before.focusable, after.focusable, "now focusable", "no longer focusable"),
    ]
    .into_iter()
    .flatten()
}

fn flag_change(before: bool, after: bool, on: &'static str, off: &'static str) -> Option<&'static str> {
    (before != after).then_some(if after { on } else { off })
}

fn option_change(before: Option<bool>, after: Option<bool>, words: [&'static str; 3]) -> Option<&'static str> {
    let [on, off, gone] = words;
    if before == after {
        return None;
    }
    Some(match after {
        Some(true) => on,
        Some(false) => off,
        None => gone,
    })
}
