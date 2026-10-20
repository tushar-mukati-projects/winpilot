use crate::{
    element::{Element, Snapshot},
    object_marks::without_object_marks,
    ocr::{NO_UIA_NOTE, OcrLine, coords},
    reports::{Found, Observation, SearchReport},
    role::Role,
};

use super::{
    NAME_LIMIT, ObserveFormat,
    element::element_line,
    quoting::{quote, single_line},
    window::{size_at, window_line, window_list, window_tag},
};

const INDENT: &str = "  ";
const NO_OCR_TEXT: &str = "(OCR found no text)";
const TREE_TRUNCATED: &str = "(tree truncated: window has more elements than the snapshot limit)";

pub fn observation(observation: &Observation, format: &ObserveFormat) -> String {
    let snapshot = &observation.snapshot;
    let listed = listed_elements(&snapshot.elements, format.all);
    let total = listed.iter().filter(|&&keep| keep).count();
    let mut lines = vec![observation_header(snapshot)];
    lines.extend(element_lines(&snapshot.elements, &listed, format));
    if total > format.max {
        lines.push(format!("… {} more elements (narrow with query, or raise max)", total - format.max));
    }
    if snapshot.truncated {
        lines.push(TREE_TRUNCATED.to_string());
    }
    lines.extend(observation.ocr.as_deref().map(ocr_lines).unwrap_or_default());
    lines.extend(window_list("other windows", &observation.others));
    lines.join("\n")
}

pub fn search(report: &SearchReport) -> String {
    let mut lines: Vec<String> = report.matches.iter().map(found_line).collect();
    if lines.is_empty() {
        lines.push("no matches".to_string());
    }
    if report.truncated {
        lines.push("… more matches (raise max)".to_string());
    }
    lines.join("\n")
}

fn ocr_lines(found: &[OcrLine]) -> Vec<String> {
    let mut lines = vec![NO_UIA_NOTE.to_string()];
    if found.is_empty() {
        lines.push(format!("{INDENT}{NO_OCR_TEXT}"));
    }
    lines.extend(
        found.iter().map(|line| format!("{INDENT}ocr {} {}", quote(&line.text, NAME_LIMIT), coords(line.rect))),
    );
    lines
}

fn observation_header(snapshot: &Snapshot) -> String {
    let focus = snapshot.focused.map(|id| format!(" · focus {id}")).unwrap_or_default();
    format!("window {} {}{focus}", window_line(&snapshot.window), size_at(snapshot.window.rect))
}

fn element_lines(elements: &[Element], listed: &[bool], format: &ObserveFormat) -> Vec<String> {
    let mut lines = Vec::new();
    let mut open_depths: Vec<u16> = Vec::new();
    for (element, &keep) in elements.iter().zip(listed) {
        if lines.len() >= format.max {
            break;
        }
        while open_depths.last().is_some_and(|&depth| depth >= element.depth) {
            open_depths.pop();
        }
        if keep {
            lines.push(format!("{}{}", INDENT.repeat(open_depths.len()), element_line(element, format.coords)));
            open_depths.push(element.depth);
        }
    }
    lines
}

fn listed_elements(elements: &[Element], all: bool) -> Vec<bool> {
    if all {
        return elements.iter().map(listed_in_full).collect();
    }
    let base: Vec<bool> = elements.iter().map(listed_by_default).collect();
    let holds_listed = holds_listed_descendant(elements, &base);
    elements
        .iter()
        .zip(base)
        .zip(holds_listed)
        .map(|((element, keep), holds)| keep || (holds && is_named_container(element)))
        .collect()
}

fn listed_in_full(element: &Element) -> bool {
    let identified = has_name(element) || has_value(element) || !element.automation_id.trim().is_empty();
    element.is_visible() && (identified || element.is_actionable())
}

fn listed_by_default(element: &Element) -> bool {
    element.is_visible() && (element.is_actionable() || is_readable_text(element))
}

fn is_readable_text(element: &Element) -> bool {
    matches!(element.role, Role::Text | Role::Document | Role::Edit) && (has_name(element) || has_value(element))
}

fn is_named_container(element: &Element) -> bool {
    element.is_visible() && element.role.is_container() && has_name(element)
}

pub(super) fn has_name(element: &Element) -> bool {
    !without_object_marks(&element.name).trim().is_empty()
}

pub(super) fn has_value(element: &Element) -> bool {
    element.value.as_deref().is_some_and(|value| !without_object_marks(value).is_empty())
}

fn holds_listed_descendant(elements: &[Element], listed: &[bool]) -> Vec<bool> {
    let mut holds = vec![false; elements.len()];
    let mut open: Vec<(usize, u16)> = Vec::new();
    for (index, element) in elements.iter().enumerate() {
        while open.last().is_some_and(|&(_, depth)| depth >= element.depth) {
            open.pop();
        }
        if listed.get(index).copied().unwrap_or(false) {
            mark_open_ancestors(&open, &mut holds);
        }
        open.push((index, element.depth));
    }
    holds
}

fn mark_open_ancestors(open: &[(usize, u16)], holds: &mut [bool]) {
    for &(ancestor, _) in open.iter().rev() {
        let Some(slot) = holds.get_mut(ancestor) else { break };
        if *slot {
            break;
        }
        *slot = true;
    }
}

fn found_line(found: &Found) -> String {
    let mut parts = vec![element_line(&found.element, false), window_tag(&found.window)];
    if !found.path.is_empty() {
        let path: Vec<String> = found.path.iter().map(|step| single_line(step.trim(), NAME_LIMIT)).collect();
        parts.push(path.join(" > "));
    }
    parts.join(" · ")
}
