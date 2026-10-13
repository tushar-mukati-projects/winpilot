use std::time::Instant;

use winpilot_core::{Element, ElementRef, Error, ErrorCode, Result, format::element_line};

use crate::{
    automation::Uia,
    fingerprint::Fingerprint,
    options::SnapshotOptions,
    resolution::{missing_ref, stale_ref},
    resolved::Resolved,
    snapshot::Walk,
};

const LISTED_CANDIDATES: usize = 5;

impl Uia {
    pub(crate) fn refind(&mut self, id: ElementRef, fingerprint: &Fingerprint) -> Result<Resolved> {
        if !fingerprint.window.is_current() {
            return Err(stale_ref(id));
        }
        let first_ref_of_rewalk = self.registry.next_ref();
        let walk = self.rewalk(id, fingerprint)?;
        let found = self.replacement_in(walk, id, fingerprint, first_ref_of_rewalk);
        self.evict_stale_refs();
        let info = found?;
        let known = self.registry.resolve(id, Instant::now()).map_err(|missing| missing_ref(id, missing))?;
        Ok(Resolved { element: known.live, info })
    }

    fn rewalk(&mut self, id: ElementRef, fingerprint: &Fingerprint) -> Result<Walk> {
        let include_offscreen = fingerprint.view().includes_offscreen();
        let options = SnapshotOptions { include_offscreen, ..SnapshotOptions::default() };
        match self.walk_window(&fingerprint.window, &options) {
            Err(error) if error.code == ErrorCode::NotFound => Err(stale_ref(id)),
            walked => walked,
        }
    }

    fn replacement_in(
        &mut self,
        walk: Walk,
        id: ElementRef,
        target: &Fingerprint,
        first_ref_of_rewalk: u32,
    ) -> Result<Element> {
        let mut elements = walk.elements;
        if let Some(position) = elements.iter().position(|element| element.id == id) {
            return Ok(elements.swap_remove(position));
        }
        match self.registry.fresh_matches(id, target, first_ref_of_rewalk).as_slice() {
            [] => Err(stale_ref(id)),
            [replacement] => self.take_over(id, *replacement, first_ref_of_rewalk, elements),
            several => Err(ambiguous_ref(id, &listed(&elements, several))),
        }
    }

    fn take_over(
        &mut self,
        stale: ElementRef,
        replacement: ElementRef,
        first_ref_of_rewalk: u32,
        elements: Vec<Element>,
    ) -> Result<Element> {
        let info = elements.into_iter().find(|element| element.id == replacement).ok_or_else(|| stale_ref(stale))?;
        if !self.registry.adopt_replacement(stale, replacement, first_ref_of_rewalk) {
            return Err(stale_ref(stale));
        }
        Ok(Element { id: stale, ..info })
    }
}

fn listed<'a>(elements: &'a [Element], ids: &[ElementRef]) -> Vec<&'a Element> {
    elements.iter().filter(|element| ids.contains(&element.id)).collect()
}

fn ambiguous_ref(id: ElementRef, candidates: &[&Element]) -> Error {
    let mut lines = vec![format!("{id} no longer exists; it may now be one of:")];
    let shown = candidates.iter().take(LISTED_CANDIDATES);
    lines.extend(shown.map(|element| format!("  {}", element_line(element, false))));
    if candidates.len() > LISTED_CANDIDATES {
        lines.push("  …".to_string());
    }
    Error::new(ErrorCode::Ambiguous, lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use winpilot_core::{ElementState, Patterns, Rect, Role};

    use super::*;

    fn row_button(id: u32) -> Element {
        Element {
            id: ElementRef(id),
            depth: 3,
            role: Role::Button,
            name: "Delete".to_string(),
            value: None,
            automation_id: String::new(),
            class_name: String::new(),
            rect: Rect::default(),
            state: ElementState { enabled: true, ..ElementState::default() },
            patterns: Patterns::default(),
        }
    }

    #[test]
    fn ambiguity_lists_the_candidates() {
        let (first, second) = (row_button(40), row_button(41));
        let error = ambiguous_ref(ElementRef(12), &[&first, &second]);
        assert_eq!(error.code, ErrorCode::Ambiguous);
        let lines: Vec<&str> = error.message.lines().collect();
        assert_eq!(lines[0], "e12 no longer exists; it may now be one of:");
        assert_eq!(lines[1], format!("  {}", element_line(&first, false)));
        assert_eq!(lines[2], format!("  {}", element_line(&second, false)));
        assert_eq!(lines.len(), 3);
        assert!(lines[1].starts_with("  e40 button"));
    }

    #[test]
    fn ambiguity_lists_at_most_five_candidates() {
        let buttons: Vec<Element> = (40..47).map(row_button).collect();
        let candidates: Vec<&Element> = buttons.iter().collect();
        let error = ambiguous_ref(ElementRef(12), &candidates);
        let lines: Vec<&str> = error.message.lines().collect();
        assert_eq!(lines.len(), 1 + LISTED_CANDIDATES + 1);
        assert!(lines[5].starts_with("  e44 "));
        assert_eq!(lines[6], "  …");
    }

    #[test]
    fn listed_candidates_keep_walk_order() {
        let elements: Vec<Element> = [50, 51, 52, 53].map(row_button).to_vec();
        let picked = listed(&elements, &[ElementRef(53), ElementRef(51)]);
        let ids: Vec<ElementRef> = picked.iter().map(|element| element.id).collect();
        assert_eq!(ids, [ElementRef(51), ElementRef(53)]);
    }
}
