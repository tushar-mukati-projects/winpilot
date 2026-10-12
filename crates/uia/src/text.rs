use deckhand_core::{ElementRef, Result, Role};
use windows::Win32::UI::Accessibility::{
    IUIAutomationTextPattern, IUIAutomationValuePattern, UIA_TextPatternId, UIA_ValuePatternId,
};

use crate::{
    automation::Uia, cached_tree::read_tree, failure::UiaResult, pattern::current_pattern, properties::cached_text,
    resolved::Resolved,
};

const MAX_TEXT_ELEMENTS: usize = 5000;

impl Uia {
    pub fn text(&mut self, id: ElementRef, max_chars: usize) -> Result<String> {
        let target = self.resolve(id)?;
        if max_chars == 0 {
            return Ok(String::new());
        }
        if let Some(text) = readable_text(&target, max_chars)? {
            return Ok(text);
        }
        self.names_inside(&target, max_chars)
    }

    fn names_inside(&self, target: &Resolved, max_chars: usize) -> Result<String> {
        let requests = self.requests.names();
        let root = read_tree(&target.element, requests).uia(&target.doing("reading the names inside"))?;
        Ok(names_in_pre_order(
            root,
            max_chars,
            |node| cached_text(unsafe { node.element.CachedName() }),
            |node| self.tree_children(node, requests),
        ))
    }
}

fn names_in_pre_order<N>(
    root: N,
    max_chars: usize,
    name_of: impl Fn(&N) -> String,
    children_of: impl Fn(&N) -> Vec<N>,
) -> String {
    let mut lines = Lines::new(max_chars);
    let mut pending = vec![root];
    for _ in 0..MAX_TEXT_ELEMENTS {
        let Some(node) = pending.pop() else { break };
        if !lines.push(&name_of(&node)) {
            break;
        }
        pending.extend(children_of(&node).into_iter().rev());
    }
    lines.finish()
}

fn readable_text(target: &Resolved, max_chars: usize) -> Result<Option<String>> {
    if target.info.state.password {
        return Ok(None);
    }
    if let Some(text) = document_text(target, max_chars)? {
        return Ok(Some(text));
    }
    if !value_is_content(target.info.role) {
        return Ok(None);
    }
    value_text(target, max_chars)
}

fn document_text(target: &Resolved, max_chars: usize) -> Result<Option<String>> {
    let reading = target.doing("reading the text of");
    let Some(pattern) = current_pattern::<IUIAutomationTextPattern>(&target.element, UIA_TextPatternId, &reading)?
    else {
        return Ok(None);
    };
    let range = unsafe { pattern.DocumentRange() }.uia(&reading)?;
    let units = i32::try_from(max_chars.saturating_mul(2)).unwrap_or(i32::MAX);
    let text = unsafe { range.GetText(units) }.uia(&reading)?;
    Ok(non_empty(truncate_chars(text.to_string(), max_chars)))
}

fn value_text(target: &Resolved, max_chars: usize) -> Result<Option<String>> {
    let reading = target.doing("reading the value of");
    let Some(pattern) = current_pattern::<IUIAutomationValuePattern>(&target.element, UIA_ValuePatternId, &reading)?
    else {
        return Ok(None);
    };
    let value = unsafe { pattern.CurrentValue() }.uia(&reading)?;
    Ok(non_empty(truncate_chars(value.to_string(), max_chars)))
}

const fn value_is_content(role: Role) -> bool {
    !matches!(role, Role::Document)
}

fn non_empty(text: String) -> Option<String> {
    (!text.is_empty()).then_some(text)
}

pub(crate) fn truncate_chars(mut text: String, max_chars: usize) -> String {
    if let Some((cut, _)) = text.char_indices().nth(max_chars) {
        text.truncate(cut);
    }
    text
}

struct Lines {
    text: String,
    previous: Option<String>,
    chars: usize,
    max_chars: usize,
}

impl Lines {
    fn new(max_chars: usize) -> Self {
        Self { text: String::new(), previous: None, chars: 0, max_chars }
    }

    fn push(&mut self, line: &str) -> bool {
        let line = line.trim();
        if !line.is_empty() && self.previous.as_deref() != Some(line) {
            if !self.text.is_empty() {
                self.text.push('\n');
                self.chars += 1;
            }
            self.text.push_str(line);
            self.chars += line.chars().count();
            self.previous = Some(line.to_string());
        }
        self.chars < self.max_chars
    }

    fn finish(self) -> String {
        truncate_chars(self.text, self.max_chars)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncates_on_char_boundaries() {
        assert_eq!(truncate_chars("héllo".to_string(), 2), "hé");
        assert_eq!(truncate_chars("hi".to_string(), 5), "hi");
        assert_eq!(truncate_chars("日本語".to_string(), 0), "");
    }

    #[test]
    fn a_document_value_is_its_address_not_its_text() {
        assert!(!value_is_content(Role::Document));
        assert!(value_is_content(Role::Edit));
        assert!(value_is_content(Role::Combobox));
    }

    #[test]
    fn lines_skip_empties_and_repeats() {
        let mut lines = Lines::new(100);
        for name in ["File", "", "  ", "File", "Edit", "File"] {
            assert!(lines.push(name));
        }
        assert_eq!(lines.finish(), "File\nEdit\nFile");
    }

    #[test]
    fn names_come_in_pre_order() {
        let children = |node: &u32| match node {
            0 => vec![1, 2],
            1 => vec![3],
            _ => Vec::new(),
        };
        assert_eq!(names_in_pre_order(0, 100, u32::to_string, children), "0\n1\n3\n2");
    }

    #[test]
    fn names_stop_at_the_element_cap() {
        let chain = |node: &usize| vec![node + 1];
        let text = names_in_pre_order(0, usize::MAX, usize::to_string, chain);
        assert_eq!(text.lines().count(), MAX_TEXT_ELEMENTS);
        assert!(text.ends_with(&format!("\n{}", MAX_TEXT_ELEMENTS - 1)));
    }

    #[test]
    fn names_stop_once_max_chars_is_reached() {
        let visited = std::cell::Cell::new(0);
        let chain = |node: &usize| {
            visited.set(visited.get() + 1);
            vec![node + 1]
        };
        assert_eq!(names_in_pre_order(10, 5, usize::to_string, chain), "10\n11");
        assert_eq!(visited.get(), 1);
    }

    #[test]
    fn lines_stop_at_the_limit() {
        let mut lines = Lines::new(6);
        assert!(lines.push("abc"));
        assert!(!lines.push("defgh"));
        assert_eq!(lines.finish(), "abc\nde");
    }
}
