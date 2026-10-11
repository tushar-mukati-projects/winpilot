use deckhand_core::{ElementRef, Result};
use windows::Win32::UI::Accessibility::{
    IUIAutomationTextPattern, IUIAutomationValuePattern, UIA_TextPatternId, UIA_ValuePatternId,
};

use crate::{automation::Uia, failure::UiaResult, properties::cached_text, resolved::Resolved, tree::cached_children};

impl Uia {
    pub fn text(&mut self, id: ElementRef, max_chars: usize) -> Result<String> {
        let target = self.resolve(id)?;
        if max_chars == 0 {
            return Ok(String::new());
        }
        if let Some(text) = readable_text(&target, max_chars)? {
            return Ok(text);
        }
        self.subtree_names(&target, max_chars)
    }

    fn subtree_names(&self, target: &Resolved, max_chars: usize) -> Result<String> {
        let cached = unsafe { target.element.BuildUpdatedCache(&self.requests.names) }
            .uia(&target.doing("reading the names inside"))?;
        let mut lines = Lines::new(max_chars);
        let mut pending = vec![cached];
        while let Some(element) = pending.pop() {
            if !lines.push(&cached_text(unsafe { element.CachedName() })) {
                break;
            }
            pending.extend(cached_children(&element).into_iter().rev());
        }
        Ok(lines.finish())
    }
}

fn readable_text(target: &Resolved, max_chars: usize) -> Result<Option<String>> {
    if target.info.state.password {
        return Ok(None);
    }
    if let Some(text) = document_text(target, max_chars)? {
        return Ok(Some(text));
    }
    value_text(target, max_chars)
}

fn document_text(target: &Resolved, max_chars: usize) -> Result<Option<String>> {
    let Some(pattern) = target.optional_pattern::<IUIAutomationTextPattern>(UIA_TextPatternId, "Text")? else {
        return Ok(None);
    };
    let reading = target.doing("reading the text of");
    let range = unsafe { pattern.DocumentRange() }.uia(&reading)?;
    let units = i32::try_from(max_chars.saturating_mul(2)).unwrap_or(i32::MAX);
    let text = unsafe { range.GetText(units) }.uia(&reading)?;
    Ok(non_empty(truncate_chars(text.to_string(), max_chars)))
}

fn value_text(target: &Resolved, max_chars: usize) -> Result<Option<String>> {
    let Some(pattern) = target.optional_pattern::<IUIAutomationValuePattern>(UIA_ValuePatternId, "Value")? else {
        return Ok(None);
    };
    let value = unsafe { pattern.CurrentValue() }.uia(&target.doing("reading the value of"))?;
    Ok(non_empty(truncate_chars(value.to_string(), max_chars)))
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
    fn lines_skip_empties_and_repeats() {
        let mut lines = Lines::new(100);
        for name in ["File", "", "  ", "File", "Edit", "File"] {
            assert!(lines.push(name));
        }
        assert_eq!(lines.finish(), "File\nEdit\nFile");
    }

    #[test]
    fn lines_stop_at_the_limit() {
        let mut lines = Lines::new(6);
        assert!(lines.push("abc"));
        assert!(!lines.push("defgh"));
        assert_eq!(lines.finish(), "abc\nde");
    }
}
